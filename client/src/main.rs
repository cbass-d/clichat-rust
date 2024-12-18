use anyhow::Result;
use std::time::Duration;
use tokio::{
    io::{self, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::broadcast::{Receiver, Sender},
};

use client::{
    state_handler::{action::Action, StateHandler},
    ClientError, ClientState, ConnectionStatus, MessageError,
};
use common::{self};
use tui::{
    app_router::AppRouter,
    components::component::{Component, ComponentRender},
    Event, Tui,
};

mod tui;

#[derive(Clone)]
enum Terminate {
    Exit,
    Error,
}

pub async fn establish_connection(server: &str) -> Result<TcpStream> {
    let stream = TcpStream::connect(server).await?;
    return Ok(stream);
}

pub fn split_stream(stream: TcpStream) -> (OwnedReadHalf, OwnedWriteHalf) {
    let (reader, writer) = stream.into_split();

    (reader, writer)
}

async fn run(shutdown_tx: Sender<Terminate>, shutdown_rx: &mut Receiver<Terminate>) -> Result<()> {
    // Initialize required strucutres:
    // * Channel for passing state between TUI and state handler
    // * Channel for passing actions/input from TUI to state handler
    // * Shutdown channels for TUI and state handler components
    let (state_handler, mut state_rx) = StateHandler::new();
    let (tui, mut action_rx, mut tui_events) = Tui::new();
    let mut shutdown_rx_state = shutdown_rx.resubscribe();
    let mut shutdown_rx_tui = shutdown_rx.resubscribe();

    // State Handler
    let state_task = tokio::spawn(async move {
        // Create and send initial state to TUI
        let mut state = ClientState::default();
        state_handler.state_tx.send(state.clone()).unwrap();

        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        let mut connection_handle: Option<(OwnedReadHalf, OwnedWriteHalf)> = None;
        let mut buf = Vec::with_capacity(4096);
        let mut update: bool = false;

        loop {
            if state.exit {
                let _ = shutdown_tx.send(Terminate::Exit);
            }

            // If a server connection exists
            if let Some((reader_stream, writer_stream)) = connection_handle.as_mut() {
                // Three sources of events:
                // * Server Tcp stream
                // * Action channel from TUI
                // * Shutdown channel
                tokio::select! {
                    _tick = ticker.tick() => {},
                    _ = reader_stream.readable() => {
                        match reader_stream.try_read_buf(&mut buf) {
                            Ok(len) if len > 0 => {
                                let raw_message = String::from_utf8(buf[0..len].to_vec()).unwrap();
                                match state.handle_message(raw_message) {
                                    Ok(_) => {}
                                    Err(e) => match e.downcast_ref() {
                                        Some(ClientError::CommandFailed { failed_cmd }) => {
                                            connection_handle = state.handle_failure(failed_cmd.to_string(), connection_handle);
                                        },
                                        _ => {},
                                    },
                                }
                            },
                            Ok(_) => {
                                let notification = String::from("[-] Connection closed by server");
                                state.terminate_connection();
                                state.push_notification(notification);
                            },
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {},
                            Err(_) => {},
                        }

                        update = true;
                        buf.clear();
                    },
                    action = action_rx.recv() => {
                        match action.unwrap() {
                            Action::SetName { name } => {
                                let message = common::Message {
                                    cmd: String::from("changename"),
                                    arg: Some(name),
                                    sender: state.username.clone(),
                                    id: state.session_id,
                                    content: None,
                                };
                                let message = common::pack_message(message);
                                state.push_notification("[*] Attemping name change".to_string());
                                let _ = writer_stream.write_all(message.as_bytes()).await;
                            },
                            Action::SendTo { room, message } => {
                                let message = common::Message {
                                    cmd: String::from("sendto"),
                                    arg: Some(room),
                                    sender: state.username.clone(),
                                    id: state.session_id,
                                    content: Some(message),
                                };
                                let message = common::pack_message(message);
                                let _ = writer_stream.write_all(message.as_bytes()).await;
                            },
                            Action::PrivMsg{ user, message } => {
                                if user == state.username {
                                    state.push_notification("[-] Cannot send message to yourself".to_string());
                                }
                                else {
                                    let message = common::Message {
                                        cmd: String::from("privmsg"),
                                        arg: Some(user),
                                        sender: state.username.clone(),
                                        id: state.session_id,
                                        content: Some(message),
                                    };
                                    let message = common::pack_message(message);
                                    let _ = writer_stream.write_all(message.as_bytes()).await;
                                }
                            },
                            Action::Join { room } => {
                                let message = common::Message {
                                    cmd: String::from("join"),
                                    arg: Some(room),
                                    sender: state.username.clone(),
                                    id: state.session_id,
                                    content: None,
                                };
                                let message = common::pack_message(message);
                                let _ = writer_stream.write_all(message.as_bytes()).await;
                            },
                            Action::Leave { room } => {
                                let message = common::Message {
                                    cmd: String::from("leave"),
                                    arg: Some(room),
                                    sender: state.username.clone(),
                                    id: state.session_id,
                                    content: None,
                                };
                                let message = common::pack_message(message);
                                let _ = writer_stream.write_all(message.as_bytes()).await;
                            }
                            Action::List { opt } => {
                                let message = common::Message {
                                    cmd: String::from("list"),
                                    arg: Some(opt),
                                    sender: state.username.clone(),
                                    id: state.session_id,
                                    content: None,
                                };
                                let message = common::pack_message(message);
                                let _ = writer_stream.write_all(message.as_bytes()).await;
                            },
                            Action::Create { room } => {
                                let message = common::Message {
                                    cmd: String::from("create"),
                                    arg: Some(room),
                                    sender: state.username.clone(),
                                    id: state.session_id,
                                    content: None,
                                };
                                let message = common::pack_message(message);
                                let _ = writer_stream.write_all(message.as_bytes()).await;
                            },
                            Action::Disconnect => {
                                state.push_notification("[-] Closing connection to server".to_string());
                                state.terminate_connection();
                                connection_handle = None;
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
                // Two sources of events:
                // * Action channel from TUI
                // * Shutdown channel
                tokio::select! {
                    _tick = ticker.tick() => {},
                    action = action_rx.recv() => {
                        match action.unwrap() {
                            Action::SetName { name } => {
                                state.username = name.clone();
                                state.push_notification(format!("[+] Name set to [{name}]"));
                            },
                            Action::Connect { addr } => {
                                if state.username.is_empty() {
                                    state.push_notification("[-] Must set a name".to_string());
                                    update = true;
                                    continue;
                                }
                                match establish_connection(&addr).await {
                                    Ok(stream) => {
                                        state.current_server = addr;
                                        state.connection_status = ConnectionStatus::Established;
                                        let _ = connection_handle.insert(split_stream(stream));
                                        state.push_notification("[+] Successfully connected".to_string());

                                        // Once connected, registration message is sent which
                                        // provides username to server
                                        state.push_notification("[*] Registering user".to_string());
                                        let message = common::Message {
                                            cmd: String::from("register"),
                                            arg: Some(state.username.clone()),
                                            sender: state.username.clone(),
                                            id: state.session_id,
                                            content: None,
                                        };
                                        let message = common::pack_message(message);
                                        if let Some((_, stream_writer)) = connection_handle.as_mut() {
                                            let _ = stream_writer.write_all(message.as_bytes()).await;
                                        }
                                        else {
                                            state.push_notification("[-] Failed to get connection handle".to_string());
                                            state.push_notification("[-] Disconnecting from server".to_string());
                                            state.terminate_connection();
                                            connection_handle = None;
                                            state.connection_status = ConnectionStatus::Unitiliazed;
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
                                state.push_notification("[-] Not connected to a server".to_string());
                            }
                        }
                        update = true;
                    },
                    _ = shutdown_rx_state.recv() => {
                            break;
                    }
                }
            }

            // Update state if needed
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

        // Main loop with three sources of events:
        // * Terminal usesr interface events such as keyboard
        // * State update channel
        // * Client shutdown channel
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

    // Wait for both tasks to finish
    let (_, _) = tokio::join!(tui_task, state_task);

    Ok(())
}

fn shutdown() {
    println!("shutting down client");
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create broadcast channel to send shutdown signal to the different components
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<Terminate>(2);
    let mut shutdown_rx_main = shutdown_rx.resubscribe();

    // Spawn thread for running app
    tokio::spawn(async move {
        let _ = run(shutdown_tx, &mut shutdown_rx).await;
    })
    .await?;

    //  Wait on shutdown signal
    tokio::select! {
        _ = shutdown_rx_main.recv() => {
            shutdown();
        }
    }

    Ok(())
}
