#![warn(clippy::all)]

mod connection;
mod state_handler;
mod tui;

use anyhow::Result;
use std::time::Duration;
use tokio::{
    io,
    net::TcpStream,
    sync::broadcast::{self},
};

use crate::state_handler::{Action, ClientState, ConnectionStatus, StateHandler};
use connection::Connection;

use common::message::{Message, MessageType};
use tui::{
    app_router::AppRouter,
    components::component::{Component, ComponentRender},
    Event, TextType, Tui,
};

#[derive(Clone)]
enum Terminate {
    Exit,
}

pub async fn establish_connection(server: &str) -> Result<TcpStream> {
    let stream = TcpStream::connect(server).await?;
    return Ok(stream);
}

pub fn display_help(state: &mut ClientState) {
    state.push_notification(TextType::Notification {
        text: String::from("List of available commands:"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /name - Set username"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /changename - Change name used in server"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /connect - Connect to server. ex. 127.0.0.1:6777"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /list {opt} - List out info. Options: users, rooms, allrooms"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /join {room} - Join room in server"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /leave {room} - Leave from room in server"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /create {room} - Create new room in server"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /sendto {room} {message}  - Send message to joined room"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /privmsg {user} {message} - Send message directly to user"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /disconnect - Disconnect from server"),
    });
    state.push_notification(TextType::Listing {
        text: String::from("    /quit - Close chat client"),
    });
}

async fn run(
    shutdown_tx: broadcast::Sender<Terminate>,
    shutdown_rx: &mut broadcast::Receiver<Terminate>,
) -> Result<()> {
    // Initialize required strucutres:
    // * Channel for passing state between TUI and state handler
    // * Channel for passing actions/input from TUI to state handler
    // * Shutdown channels for TUI and state handler components
    let (state_handler, mut state_rx) = StateHandler::new();
    let (tui, mut action_rx, mut tui_events) = Tui::new();
    let mut shutdown_state = shutdown_rx.resubscribe();
    let mut shutdown_tui = shutdown_rx.resubscribe();

    // State Handler
    let state_task = tokio::spawn(async move {
        // Create and send initial state to TUI
        let mut state = ClientState::default();
        state_handler.send_update(state.clone());

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
                                state.push_notification(TextType::Error {
                                    text: String::from("[-] Closed connection to server"),
                                });
                                state.terminate_connection();
                                connection_handle = None;
                            },
                        }

                        update = true;
                    },
                    action = action_rx.recv() => {
                        match action.unwrap() {
                            Action::Help => {
                                    display_help(&mut state);
                                    update = true;
                            },
                            Action::SetName { name } => {

                                let message = Message::build(
                                        MessageType::ChangeName,
                                        state.session_id,
                                        Some(name),
                                        None
                                    );

                                state.push_notification(TextType::Notification {
                                        text: String::from("[*] Attemping name change")
                                });

                                let _ = connection.write(message).await;
                            },
                            Action::SendTo { room, message } => {

                                let message = Message::build(
                                        MessageType::SendTo,
                                        state.session_id,
                                        Some(room),
                                        Some(message),
                                    );

                                let _ = connection.write(message).await;
                            },
                            Action::PrivMsg{ user, message } => {
                                if user == state.username {
                                    state.push_notification(TextType::Error {
                                            text: String::from("[-] Cannot send message to yourself"),
                                    });

                                    update = true;
                                }
                                else {

                                    let message = Message::build(
                                            MessageType::PrivMsg,
                                            state.session_id,
                                            Some(user),
                                            Some(message),
                                        );

                                    let _ = connection.write(message).await;
                                }
                            },
                            Action::Join { room } => {

                                let message = Message::build(
                                        MessageType::Join,
                                        state.session_id,
                                        Some(room),
                                        None,
                                    );

                                let _ = connection.write(message).await;
                            },
                            Action::Leave { room } => {

                                let message = Message::build(
                                        MessageType::Leave,
                                        state.session_id,
                                        Some(room),
                                        None,
                                    );

                                let _ = connection.write(message).await;
                            },
                            Action::List { opt } => {

                                let message = Message::build(
                                        MessageType::List,
                                        state.session_id,
                                        Some(opt),
                                        None,
                                    );

                                let _ = connection.write(message).await;
                            },
                            Action::Create { room } => {

                                let message = Message::build(
                                        MessageType::Create,
                                        state.session_id,
                                        Some(room),
                                        None,
                                    );

                                let _ = connection.write(message).await;
                            },
                            Action::Disconnect => {
                                state.push_notification(TextType::Notification {
                                        text: String::from("[-] Closing connection to server"),
                                });
                                state.terminate_connection();
                                connection_handle = None;

                                update = true;
                            },
                            Action::Quit => {
                                state.exit();
                            },
                            Action::Invalid => {
                                state.push_notification(TextType::Error {
                                        text: String::from("[-] Invalid command")
                                });
                                update = true;
                            },
                            _ => {}
                        }
                    },
                    _ = shutdown_state.recv() => {
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
                            Action::Help => {
                                display_help(&mut state);
                            },
                            Action::SetName { name } => {
                                state.username = name.clone();
                                state.push_notification(TextType::Notification {
                                    text: format!("[*] Name set to [{name}]"),
                                });
                            },
                            Action::Connect { addr } => {
                                if state.username.is_empty() {
                                    state.push_notification(TextType::Error {
                                        text: String::from("[-] Must set a name"),
                                    });
                                    update = true;
                                    continue;
                                }

                                match establish_connection(&addr).await {
                                    Ok(stream) => {
                                        state.current_server = addr;
                                        state.connection_status = ConnectionStatus::Established;
                                        let connection = Connection::new(stream);
                                        let _ = connection_handle.insert(connection);
                                        state.push_notification(TextType::Notification {
                                            text: String::from("[*] Successfully connected"),
                                        });

                                        // Once connected, registration message is sent which
                                        // provides username to server
                                        state.push_notification(TextType::Notification {
                                            text: String::from("[*] Registering user"),
                                        });

                                        let message = Message::build(
                                                MessageType::Register,
                                                state.session_id,
                                                Some(state.username.clone()),
                                                None,
                                            );

                                        if let Some(connection) = connection_handle.as_mut() {
                                            let _ = connection.write(message).await;
                                        }
                                        else {
                                            state.push_notification(TextType::Error {
                                                text: String::from("[-] Failed to get connection handle"),
                                            });
                                            state.push_notification(TextType::Error {
                                                text: String::from("[-] Disconnecting from server"),
                                            });
                                            state.terminate_connection();
                                            connection_handle = None;
                                            state.connection_status = ConnectionStatus::Unitiliazed;
                                        }
                                    },
                                    Err(e) => {
                                        let err = e.to_string();
                                        state.push_notification(TextType::Error {
                                            text: format!("[-] Failed to connect: {err}"),
                                        });
                                    },
                                }
                            },
                            Action::Quit => {
                                state.exit();
                            },
                            Action::Invalid => {
                                state.push_notification(TextType::Error {
                                    text: String::from("[-] Invalid command"),
                                });
                            },
                            _ => {
                                state.push_notification(TextType::Error {
                                    text: String::from("[-] Not connected to a server")
                                });
                            }
                        }

                        update = true;
                    },
                    _ = shutdown_state.recv() => {
                            break;
                    }
                }
            }

            // Update state if needed
            if update {
                state_handler.send_update(state.clone());
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
                _ = shutdown_tui.recv() => {
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
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<Terminate>(2);
    let mut shutdown_main = shutdown_rx.resubscribe();

    tokio::spawn(async move {
        let _ = run(shutdown_tx, &mut shutdown_rx).await;
    })
    .await?;

    tokio::select! {
        _ = shutdown_main.recv() => {
            shutdown();
        }
    }

    Ok(())
}
