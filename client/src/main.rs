use std::time::Duration;
use tokio::{
    io::*,
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::broadcast::{Receiver, Sender},
};

use common::{self, Message};
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
                                if let Some(message) = common::unpack_message(&raw_message) {
                                    match message.cmd.as_str() {
                                        "roommessage" => {
                                            let room = message.arg.unwrap();
                                            let content = message.content.unwrap();
                                            state.push_notification(format!("[#{room}] {0} #{1}: {content}", message.sender, message.id));
                                        },
                                        "message" => {
                                            let content = message.content.unwrap();
                                            state.push_notification(format!("[privmsg] {0} #{1}: {content}", message.sender, message.id));
                                        }
                                        "joined" => {
                                            let notification = format!("[+] Joined [{0}] room", message.arg.unwrap());
                                            state.push_notification(notification);
                                        },
                                        "left" => {
                                            let message = message.content.unwrap();
                                            state.push_notification(message.to_string());
                                        },
                                        "rooms" => {
                                            if let Some(message) = message.content {
                                                let mut rooms: Vec<&str> = Vec::new();
                                                if message.contains(',') {
                                                    rooms = message.split(',').collect();
                                                } else {
                                                    rooms.push(&message);
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
                                            if let Some(message) = message.content {
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
                                            let notification = format!("[+] Registered as [{0}]", message.arg.clone().unwrap());
                                            state.push_notification(notification);
                                            state.set_name(message.arg.unwrap());
                                            state.set_session_id(message.content.unwrap().parse::<u64>().unwrap());
                                            state.set_as_registered();
                                        }
                                        "changedname" => {
                                            match message.arg.unwrap().as_str() {
                                                "failed" => {
                                                    let message = message.content.unwrap();
                                                    state.push_notification(message.to_string());
                                                }
                                                new_name => {
                                                    state.set_name(new_name.to_string());
                                                    let message = message.content.unwrap();
                                                    state.push_notification(message.to_string());
                                                }
                                                _ => {}
                                            }
                                        },
                                        "created" => {
                                            match message.arg.unwrap().as_str() {
                                                "success" => {
                                                    state.push_notification("[+] Successfully created room".to_string());
                                                },
                                                "failed" => {
                                                    state.push_notification("[-] Failed to create room".to_string());
                                                },
                                                _ => {}
                                            }
                                        }
                                        "failed" => {
                                            let notification = format!("[-] {0} failed: {1}", message.arg.clone().unwrap(), message.content.unwrap());
                                            state.push_notification(notification);
                                            match message.arg.unwrap().as_str() {
                                                "register" => {
                                                    (state, connection_handle) = terminate_connection(&mut state);
                                                    state.push_notification("[-] Closing connection to server".to_string());
                                                }
                                                _ => {},
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
                                let message = common::Message {
                                    cmd: String::from("name"),
                                    arg: Some(name),
                                    sender: state.get_name(),
                                    id: state.get_session_id(),
                                    content: None,
                                };
                                let message = common::pack_message(message);
                                state.push_notification("[*] Attemping name change".to_string());
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            },
                            Action::SendTo { room, message } => {
                                let message = common::Message {
                                    cmd: String::from("sendto"),
                                    arg: Some(room),
                                    sender: state.get_name(),
                                    id: state.get_session_id(),
                                    content: Some(message),
                                };
                                let message = common::pack_message(message);
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            },
                            Action::PrivMsg{ user, message } => {
                                if user == state.get_name() {
                                    state.push_notification("[-] Cannot send message to yourself".to_string());
                                }
                                else {
                                    let message = common::Message {
                                        cmd: String::from("privmsg"),
                                        arg: Some(user),
                                        sender: state.get_name(),
                                        id: state.get_session_id(),
                                        content: Some(message),
                                    };
                                    let message = common::pack_message(message);
                                    let _ = req_handler.writer.write_all(message.as_bytes()).await;
                                }
                            },
                            Action::Join { room } => {
                                let message = common::Message {
                                    cmd: String::from("join"),
                                    arg: Some(room),
                                    sender: state.get_name(),
                                    id: state.get_session_id(),
                                    content: None,
                                };
                                let message = common::pack_message(message);
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            },
                            Action::Leave { room } => {
                                let message = common::Message {
                                    cmd: String::from("leave"),
                                    arg: Some(room),
                                    sender: state.get_name(),
                                    id: state.get_session_id(),
                                    content: None,
                                };
                                let message = common::pack_message(message);
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            }
                            Action::List { opt } => {
                                let message = common::Message {
                                    cmd: String::from("list"),
                                    arg: Some(opt),
                                    sender: state.get_name(),
                                    id: state.get_session_id(),
                                    content: None,
                                };
                                let message = common::pack_message(message);
                                let _ = req_handler.writer.write_all(message.as_bytes()).await;
                            },
                            Action::Create { room } => {
                                let message = common::Message {
                                    cmd: String::from("create"),
                                    arg: Some(room),
                                    sender: state.get_name(),
                                    id: state.get_session_id(),
                                    content: None,
                                };
                                let message = common::pack_message(message);
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
                            },
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
                                        let message = common::Message {
                                            cmd: String::from("register"),
                                            arg: Some(state.get_name()),
                                            sender: state.get_name(),
                                            id: state.get_session_id(),
                                            content: None,
                                        };
                                        let message = common::pack_message(message);
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
                            Action::Quit => {
                                state.exit();
                            },
                            Action::Invalid => {
                                state.push_notification("[-] Invalid command".to_string());
                            },
                            _ => {
                                state.push_notification("[-] Not connected to a serve".to_string());
                            }
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
