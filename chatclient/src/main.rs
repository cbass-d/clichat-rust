mod connection;
mod state_handler;
mod tui;

use anyhow::Result;
use std::time::Duration;
use tokio::{
    io,
    net::TcpStream,
    sync::broadcast::{Receiver, Sender},
};

use crate::state_handler::{Action, ClientState, ConnectionStatus, StateHandler};
use connection::Connection;

use common::message::{Message, MessageBody, MessageHeader, MessageType};
use tui::{
    app_router::AppRouter,
    components::component::{Component, ComponentRender},
    Event, Tui,
};

#[derive(Clone)]
enum Terminate {
    Exit,
    Error,
}

pub async fn establish_connection(server: &str) -> Result<TcpStream> {
    let stream = TcpStream::connect(server).await?;
    return Ok(stream);
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
        let mut connection_handle: Option<Connection> = None;
        let mut update: bool = false;

        loop {
            if state.exit {
                let _ = shutdown_tx.send(Terminate::Exit);
            }

            if let Some(connection) = connection_handle.as_mut() {
                // Three sources of events:
                // * Server Tcp stream
                // * Action channel from TUI
                // * Shutdown channel
                tokio::select! {
                    _tick = ticker.tick() => {},
                    read_result = connection.read() => {
                        match read_result {
                            Ok(message) => {

                                // Certain errors need the connection to be closed
                                let _ = state.handle_message(message).map_err(|_| {
                                    connection_handle = None;
                                });

                            },
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {},
                            Err(_) => {
                                state.push_notification(String::from("[-] Closed connection to server"));
                                state.terminate_connection();
                                connection_handle = None;
                            },
                        }

                        update = true;
                    },
                    action = action_rx.recv() => {
                        match action.unwrap() {
                            Action::SetName { name } => {
                                let header = MessageHeader {
                                        message_type: MessageType::ChangeName,
                                        sender_id: state.session_id,
                                };
                                let body = MessageBody {
                                        arg: Some(name),
                                        content: None,
                                };
                                let message = Message {
                                        header,
                                        body,
                                };

                                state.push_notification("[*] Attemping name change".to_string());

                                let _ = connection.write(message).await;
                            },
                            Action::SendTo { room, message } => {
                                let header = MessageHeader {
                                        message_type: MessageType::SendTo,
                                        sender_id: state.session_id,
                                };
                                let body = MessageBody {
                                        arg: Some(room),
                                        content: Some(message),
                                };
                                let message = Message {
                                        header,
                                        body,
                                };

                                let _ = connection.write(message).await;
                            },
                            Action::PrivMsg{ user, message } => {
                                if user == state.username {
                                    state.push_notification("[-] Cannot send message to yourself".to_string());

                                    update = true;
                                }
                                else {
                                    let header = MessageHeader {
                                            message_type: MessageType::PrivMsg,
                                            sender_id: state.session_id,
                                    };
                                    let body = MessageBody {
                                            arg: Some(user),
                                            content: Some(message),
                                    };
                                    let message = Message {
                                            header,
                                            body,
                                    };

                                    let _ = connection.write(message).await;
                                }
                            },
                            Action::Join { room } => {
                                let header = MessageHeader {
                                        message_type: MessageType::Join,
                                        sender_id: state.session_id,
                                };
                                let body = MessageBody {
                                        arg: Some(room),
                                        content: None,
                                };
                                let message = Message {
                                        header,
                                        body,
                                };

                                let _ = connection.write(message).await;
                            },
                            Action::Leave { room } => {
                                let header = MessageHeader {
                                        message_type: MessageType::Leave,
                                        sender_id: state.session_id,
                                };
                                let body = MessageBody {
                                        arg: Some(room),
                                        content: None,
                                };
                                let message = Message {
                                        header,
                                        body,
                                };

                                let _ = connection.write(message).await;
                            },
                            Action::List { opt } => {
                                let header = MessageHeader {
                                        message_type: MessageType::List,
                                        sender_id: state.session_id,
                                };
                                let body = MessageBody {
                                        arg: Some(opt),
                                        content: None,
                                };
                                let message = Message {
                                        header,
                                        body,
                                };

                                let _ = connection.write(message).await;
                            },
                            Action::Create { room } => {
                                let header = MessageHeader {
                                        message_type: MessageType::Create,
                                        sender_id: state.session_id,
                                };
                                let body = MessageBody {
                                        arg: Some(room),
                                        content: None,
                                };
                                let message = Message {
                                        header,
                                        body,
                                };

                                let _ = connection.write(message).await;
                            },
                            Action::Disconnect => {
                                state.push_notification("[-] Closing connection to server".to_string());
                                state.terminate_connection();
                                connection_handle = None;

                                update = true;
                            },
                            Action::Quit => {
                                state.exit();
                            },
                            Action::Invalid => {
                                state.push_notification("[-] Invalid command".to_string());
                                update = true;
                            },
                            _ => {}
                        }
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
                                        let connection = Connection::new(stream);
                                        let _ = connection_handle.insert(connection);
                                        state.push_notification("[+] Successfully connected".to_string());

                                        // Once connected, registration message is sent which
                                        // provides username to server
                                        state.push_notification("[*] Registering user".to_string());
                                        let header = MessageHeader {
                                            message_type: MessageType::Register,
                                            sender_id: state.session_id,
                                        };
                                        let body = MessageBody {
                                            arg: Some(state.username.clone()),
                                            content: None,
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        if let Some(connection) = connection_handle.as_mut() {
                                            let _ = connection.write(message).await;
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
                },
                state = state_rx.recv() => {
                    match state {
                        Some(state) => {
                            let _ = terminal.clear();
                            app_router = app_router.update(&state);
                        }
                        None => {},
                    }
                },
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
