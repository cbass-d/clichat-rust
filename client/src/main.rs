use std::time::Duration;
use tokio::{
    io::*,
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::broadcast::{Receiver, Sender},
};

use common;
use state_handler::{
    action::Action,
    state::{ConnectionStatus, State},
    StateHandler,
};
use tui::{
    app_router::AppRouter,
    components::component::{Component, ComponentRender},
    Event, Tui,
};

mod state_handler;
mod tui;

struct ResponseStream {
    reader: OwnedReadHalf,
}

struct RequestHandler {
    writer: OwnedWriteHalf,
}

#[derive(Clone)]
enum Terminate {
    StateExit,
    Error,
}

type ConnectionHandler = (ResponseStream, RequestHandler);

fn split_stream(stream: TcpStream) -> ConnectionHandler {
    let (reader, writer) = stream.into_split();

    (ResponseStream { reader }, RequestHandler { writer })
}

async fn establish_connection(server: &str) -> Result<TcpStream> {
    let stream = TcpStream::connect(server).await?;
    return Ok(stream);
}

fn terminate_connection(state: &mut State) -> (State, Option<ConnectionHandler>) {
    state.set_connection_status(ConnectionStatus::Unitiliazed);

    (state.clone(), None)
}

async fn run(shutdown_tx: Sender<Terminate>, shutdown_rx: &mut Receiver<Terminate>) -> Result<()> {
    let (state_handler, mut state_rx) = StateHandler::new();
    let (tui, mut action_rx, mut tui_events) = Tui::new();
    let mut shutdown_rx_state = shutdown_rx.resubscribe();
    let mut shutdown_rx_tui = shutdown_rx.resubscribe();

    // State Handler
    let state_task = tokio::spawn(async move {
        // Create and send initial state to TUI
        let mut state = State::default();
        state_handler.state_tx.send(state.clone()).unwrap();

        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        let mut connection_handle: Option<ConnectionHandler> = None;
        let mut buf = Vec::with_capacity(4096);
        let mut update: bool = false;

        loop {
            if state.exit {
                let _ = shutdown_tx.send(Terminate::StateExit);
            }

            if let Some((res_stream, req_handler)) = connection_handle.as_mut() {
                tokio::select! {
                    _tick = ticker.tick() => {},
                    _ = res_stream.reader.readable() => {
                        match res_stream.reader.try_read_buf(&mut buf) {
                            Ok(len) if len > 0 => {
                                let raw_message = String::from_utf8(buf[0..len].to_vec()).unwrap();
                                if let Some((cmd, arg, sender, id, message)) = common::unpack_message(&raw_message) {
                                    match cmd {
                                        "roommessage" => {
                                            let room = arg.unwrap();
                                            let message = message.unwrap();
                                            state.push_notification(format!("[#{room}] {sender} #{id}: {message}"));
                                        },
                                        "joined" => {
                                            let message = message.unwrap();
                                            state.push_notification(message.to_string());
                                        },
                                        "rooms" => {
                                            if let Some(message) = message {
                                                let mut rooms: Vec<&str> = Vec::new();
                                                if message.contains(',') {
                                                    rooms = message.split(',').collect();
                                                } else {
                                                    rooms.push(message);
                                                }
                                                state.push_notification("[+] List of rooms:".to_string());
                                                for room in rooms {
                                                    state.push_notification(format!("[#{room}]"));
                                                }
                                                state.push_notification("[+] End of rooms".to_string());
                                            }
                                            else {
                                                state.push_notification("[*] No joined rooms".to_string());
                                            }
                                        },
                                        "users" => {
                                            if let Some(message) = message {
                                                let mut user_list: Vec<String> = Vec::new();
                                                if message.contains(',') {
                                                    user_list = message.split(',').map(|s| s.to_string()).collect();
                                                    user_list = user_list.into_iter().map(|user_id| {
                                                        let user_id: Vec<String> = user_id.split(' ').map(|s| s.to_string()).collect();
                                                        format!("{} #{}", user_id[0], user_id[1])
                                                    }).collect();
                                                } else {
                                                    let user_id: Vec<String> = message.split(' ').map(|s| s.to_string()).collect();
                                                    user_list.push(format!("{0} #{1}", user_id[0], user_id[1]));
                                                }
                                                state.push_notification("[+] Users in server:".to_string());
                                                for user in user_list {
                                                    state.push_notification(format!("[{user}]"));
                                                }
                                                state.push_notification("[+] End of users".to_string());
                                            }
                                            else {
                                                state.push_notification("[*] No users on server".to_string());
                                            }
                                        },
                                        "registered" => {
                                            match arg {
                                                Some("success") => {
                                                    let id: u64 = message.unwrap().parse().unwrap();
                                                    state.set_session_id(id);
                                                    state.set_as_registered();
                                                    state.push_notification(format!("[+] Registered as {0} #{1}", state.get_name(), id));
                                                },
                                                Some("taken") => {
                                                    (state, connection_handle) = terminate_connection(&mut state);
                                                    state.push_notification("[-] Name is already taken".to_string());
                                                    state.push_notification("[-] Closing connection to server".to_string());
                                                    state.push_notification("[*] Change name with \"/name\" command and reconnect".to_string());
                                                },
                                                Some("failed") => {
                                                    (state, connection_handle) = terminate_connection(&mut state);
                                                    state.push_notification("[-] Failed to register user".to_string());
                                                    state.push_notification("[-] Closing connection to server".to_string());
                                                },
                                                _ => {},
                                            }
                                        }
                                        "changedname" => {
                                            match arg {
                                                Some("failed") => {
                                                    let message = message.unwrap();
                                                    state.push_notification(message.to_string());
                                                }
                                                Some(new_name) => {
                                                    state.set_name(new_name.to_string());
                                                    let message = message.unwrap();
                                                    state.push_notification(message.to_string());
                                                }
                                                _ => {}
                                            }
                                        },
                                        "created" => {
                                            match arg {
                                                Some("success") => {
                                                    state.push_notification("[+] Successfully created room".to_string());
                                                },
                                                Some("failed") => {
                                                    state.push_notification("[-] Failed to create room".to_string());
                                                },
                                                _ => {}
                                            }
                                        }
                                        _ => {},
                                    }
                                }
                                update = true;
                            },
                            Ok(_) => {
                                state.push_notification("[-] Connection closed by server".to_string());
                                (state, connection_handle) = terminate_connection(&mut state);
                                update = true;
                            },
                            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {},
                            Err(e) => {
                                let err = e.to_string();
                                state.push_notification("[-] Failed to read from stream: ".to_string() + &err);
                                state.push_notification("[-] Closing connection to server".to_string());
                                (state, connection_handle) = terminate_connection(&mut state);
                                update = true;
                            }
                        }
                        buf.clear();
                    }
                    action = action_rx.recv() => {
                        match action.unwrap() {
                            Action::SetName { name } => {
                                let message = common::pack_message("name", Some(&name), &state.get_name(), state.get_session_id(), None);
                                state.push_notification("[*] Attemping name change".to_string());
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            },
                            Action::SendTo { arg, message } => {
                                let message = common::pack_message("sendto", Some(&arg), &state.get_name(), state.get_session_id(), Some(&message));
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            },
                            Action::Join { room } => {
                                let message = common::pack_message("join", Some(&room), &state.get_name(), state.get_session_id(), None);
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            },
                            Action::List { opt } => {
                                let message = common::pack_message("list", Some(&opt), &state.get_name(), state.get_session_id(), None);
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            },
                            Action::Create { room } => {
                                let message = common::pack_message("create", Some(&room), &state.get_name(), state.get_session_id(), None);
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            },
                            Action::Disconnect => {
                                state.push_notification("[-] Closing connection to server".to_string());
                                (state, connection_handle) = terminate_connection(&mut state);
                            },
                            Action::Quit => {
                                state.exit();
                            },
                            Action::Invalid => {
                                state.push_notification("[-] Invalid command".to_string());
                            },
                            _ => {}
                        }
                        update = true;
                    },
                    _ = shutdown_rx_state.recv() => {
                        break;
                    }
                }
            } else {
                tokio::select! {
                    _tick = ticker.tick() => {},
                    action = action_rx.recv() => {
                        match action.unwrap() {
                            Action::SetName { name } => {
                                state.set_name(name.clone());
                                state.push_notification(format!("[+] Name set to [{name}]"));
                            }
                            Action::SendTo {..} => {
                                state.push_notification("[-] Not connected to a server".to_string());
                            }
                            Action::Join {..} => {
                                state.push_notification("[-] Not connected to a server".to_string());
                            }
                            Action::List {..} => {
                                state.push_notification("[-] Not connected to a server".to_string());
                            }
                            Action::Connect { addr } => {
                                if state.get_name().is_empty() {
                                    state.push_notification("[-] Must set name".to_string());
                                    update = true;
                                    continue;
                                }
                                match establish_connection(&addr).await {
                                    Ok(s) => {
                                        state.set_server(addr);
                                        state.set_connection_status(ConnectionStatus::Established);
                                        let _ = connection_handle.insert(split_stream(s));
                                        state.push_notification("[+] Successfully connected".to_string());

                                        // Once connected, registration message is sent which
                                        // includes username
                                        state.push_notification("[*] Registering user".to_string());
                                        let message = common::pack_message("register", Some(&state.get_name()), &state.get_name(), 0, None);
                                        if let Some((_, res_writer)) = connection_handle.as_mut() {
                                            let _ = res_writer.writer.write_all(message.as_bytes()).await;
                                        }
                                        else {
                                            state.push_notification("[-] Failed to get connection handle".to_string());
                                            state.push_notification("[-] Disconnecting from server".to_string());
                                            state.set_connection_status(ConnectionStatus::Unitiliazed);
                                        }
                                    },
                                    Err(e) => {
                                        let err = e.to_string();
                                        state.push_notification("[-] Failed to connect: ".to_string() + &err);
                                    },
                                }
                            },
                            Action::Disconnect => {
                                state.push_notification("[-] Not connected to a server".to_string());
                            },
                            Action::Create {..} => {
                                state.push_notification("[-] Not connected to a server".to_string());
                            },
                            Action::Quit => {
                                state.exit();
                            },
                            Action::Invalid => {
                                state.push_notification("[-] Invalid command".to_string());
                            },
                        }
                        update = true;
                    },
                    _ = shutdown_rx_state.recv() => {
                            break;
                    }
                }
            }

            if update {
                match state_handler.state_tx.send(state.clone()) {
                    Ok(_) => {}
                    Err(_) => {
                        let _ = shutdown_tx.send(Terminate::Error);
                    }
                }
                update = false;
            }
        }
    });

    // TUI Handler
    let tui_task = tokio::spawn(async move {
        let mut terminal = Tui::setup_terminal();
        // Get initial State
        let state = state_rx.recv().await.unwrap();
        let mut app_router = AppRouter::new(&state, tui.action_tx);
        let _ = terminal.draw(|f| app_router.render(f, ()));

        loop {
            tokio::select! {
                event = tui_events.next() => {
                    match event.unwrap() {
                        Event::Key(key) => {
                            app_router.handle_key_event(key);
                        },
                        Event::Tick => {},
                        Event::Error => {},
                    }
                }
                state = state_rx.recv() => {
                    match state {
                        Some(state) => {
                            let _ = terminal.clear();
                            app_router = app_router.update(&state);
                        }
                        None => {},
                    }
                }
                _ = shutdown_rx_tui.recv() => {
                    break;
                }
            }

            let _ = terminal.draw(|f| app_router.render(f, ()));
        }

        Tui::teardown_terminal(&mut terminal);
    });

    let (_, _) = tokio::join!(tui_task, state_task);

    Ok(())
}

fn shutdown() {
    println!("shutting down client");
}

#[tokio::main]
async fn main() -> Result<()> {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<Terminate>(2);
    let mut shutdown_rx_main = shutdown_rx.resubscribe();
    tokio::spawn(async move {
        let _ = run(shutdown_tx, &mut shutdown_rx).await;
    })
    .await?;

    tokio::select! {
        _ = shutdown_rx_main.recv() => {
            shutdown();
        }
    }

    Ok(())
}
