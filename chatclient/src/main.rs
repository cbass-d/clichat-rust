#![warn(clippy::all)]

mod connection;
mod state_handler;
mod tui;

use anyhow::Result;
use std::sync::{Arc, Mutex};
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
    Ok(stream)
}

pub async fn registering_on_server(
    server: &str,
    state: Arc<Mutex<ClientState>>,
    connection_handle: &mut Option<Connection>,
) -> Result<()> {
    let stream = establish_connection(server).await?;

    {
        let mut state = state.lock().unwrap();
        state.current_server = server.to_string();
        state.connection_status = ConnectionStatus::Established;
    }
    let mut connection = Connection::new(stream);

    {
        let mut state = state.lock().unwrap();
        state.push_notification(TextType::Notification {
            text: String::from("[*] Successfully connected"),
        });

        // Once connected, registration message is sent which
        // provides username to server
        state.push_notification(TextType::Notification {
            text: String::from("[*] Registering user"),
        });
    }

    let (session_id, username) = {
        let guard = state.lock().unwrap();
        (guard.session_id.clone(), guard.username.clone())
    };

    if let Ok(message) = Message::build(MessageType::Register, session_id, Some(username), None) {
        let _ = connection.write(message).await;
    }

    let _ = connection_handle.insert(connection);

    Ok(())
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
    master_state: Arc<Mutex<ClientState>>,
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
    let state_task = tokio::spawn({
        let handler_state = Arc::clone(&master_state);
        let mut exit = false;

        async move {
            // Set up initial TUI state
            state_handler.updated();

            let mut ticker = tokio::time::interval(Duration::from_millis(250));
            let mut connection_handle: Option<Connection> = None;

            loop {
                if exit {
                    let _ = shutdown_tx.send(Terminate::Exit);
                    connection_handle = None;
                }

                if let Some(connection) = connection_handle.as_mut() {
                    // Three sources of events:
                    // * Server Tcp stream
                    // * Action channel from TUI
                    // * Shutdown channel
                    tokio::select! {
                        _tick = ticker.tick() => {},
                        _ = connection.readable() => {
                                match connection.read().await {
                                    Ok(message) => {
                                        let mut handler_state = handler_state.lock().unwrap();

                                        if let Some(message) = message {
                                            // Certain errors need the connection to be closed
                                            let _ = handler_state.handle_message(message).map_err(|_| {
                                                connection_handle = None;
                                            });
                                        }

                                    },
                                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {},
                                    Err(_) => {
                                        let mut handler_state = handler_state.lock().unwrap();

                                        handler_state.push_notification(TextType::Error {
                                            text: String::from("[-] Closed connection to server"),
                                        });

                                        handler_state.terminate_connection();
                                        connection_handle = None;
                                    },
                                }

                            state_handler.updated();
                        },
                        action = action_rx.recv() => {
                            match action {
                                Some(Action::Help) => {
                                    let mut handler_state = handler_state.lock().unwrap();

                                    display_help(&mut handler_state);
                                },
                                Some(Action::SetName { name }) => {
                                    let session_id = {
                                            let guard = handler_state.lock().unwrap();
                                            guard.session_id
                                    };

                                    if let Ok(message) = Message::build(
                                            MessageType::ChangeName,
                                            session_id,
                                            Some(name),
                                            None
                                    ) {
                                        let _ = connection.write(message).await;
                                    }


                                    let mut handler_state = handler_state.lock().unwrap();

                                    handler_state.push_notification(TextType::Notification {
                                            text: String::from("[*] Attemping name change")
                                    });

                                },
                                Some(Action::SendTo { room, message }) => {
                                    let session_id = {
                                            let guard = handler_state.lock().unwrap();
                                            guard.session_id
                                    };

                                    if let Ok(message) = Message::build(
                                            MessageType::SendTo,
                                            session_id,
                                            Some(room),
                                            Some(message),
                                        ) {
                                            let _ = connection.write(message).await;
                                        }

                                },
                                Some(Action::PrivMsg{ user, message }) => {
                                    let (session_id, username) = {
                                            let guard = handler_state.lock().unwrap();
                                            (guard.session_id, guard.username.clone())
                                    };

                                    if user == username {
                                        let mut handler_state = handler_state.lock().unwrap();
                                        handler_state.push_notification(TextType::Error {
                                                text: String::from("[-] Cannot send message to yourself"),
                                        });
                                    }

                                    else {

                                        if let Ok(message) = Message::build(
                                                MessageType::PrivMsg,
                                                session_id,
                                                Some(user),
                                                Some(message),
                                            ) {
                                                let _ = connection.write(message).await;
                                            }

                                    }
                                },
                                Some(Action::Join { room }) => {
                                    let session_id = {
                                        let guard = handler_state.lock().unwrap();
                                        guard.session_id
                                    };

                                    if let Ok(message) = Message::build(
                                            MessageType::Join,
                                            session_id,
                                            Some(room),
                                            None,
                                        ) {
                                            let _ = connection.write(message).await;
                                        }

                                },
                                Some(Action::Leave { room }) => {
                                    let session_id = {
                                        let guard = handler_state.lock().unwrap();
                                        guard.session_id
                                    };

                                    if let Ok(message) = Message::build(
                                            MessageType::Leave,
                                            session_id,
                                            Some(room),
                                            None,
                                        ) {
                                            let _ = connection.write(message).await;
                                        }

                                },
                                Some(Action::List { opt }) => {
                                    let session_id = {
                                        let guard = handler_state.lock().unwrap();
                                        guard.session_id
                                    };

                                    if let Ok(message) = Message::build(
                                            MessageType::List,
                                            session_id,
                                            Some(opt),
                                            None,
                                        ) {
                                            let _ = connection.write(message).await;
                                        }

                                },
                                Some(Action::Create { room }) => {
                                    let session_id = {
                                        let guard = handler_state.lock().unwrap();
                                        guard.session_id
                                    };

                                    if let Ok(message) = Message::build(
                                            MessageType::Create,
                                            session_id,
                                            Some(room),
                                            None,
                                        ) {
                                            let _ = connection.write(message).await;
                                        }

                                },
                                Some(Action::Disconnect) => {
                                    let mut handler_state = handler_state.lock().unwrap();

                                    handler_state.push_notification(TextType::Notification {
                                            text: String::from("[-] Closing connection to server"),
                                    });

                                    handler_state.terminate_connection();
                                    connection_handle = None;
                                },
                                Some(Action::Quit) => {
                                    let mut handler_state = handler_state.lock().unwrap();
                                    handler_state.exit();
                                    exit = true;
                                },
                                Some(Action::Invalid) => {
                                    let mut handler_state = handler_state.lock().unwrap();

                                    handler_state.push_notification(TextType::Error {
                                            text: String::from("[-] Invalid command")
                                    });
                                },
                                _ => {}
                            }

                            state_handler.updated();
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
                            match action {
                                Some(Action::Help) => {
                                    let mut handler_state = handler_state.lock().unwrap();

                                    display_help(&mut handler_state);
                                },
                                Some(Action::SetName { name }) => {
                                    let mut handler_state = handler_state.lock().unwrap();

                                    handler_state.username = name.clone();
                                    handler_state.push_notification(TextType::Notification {
                                        text: format!("[*] Name set to [{name}]"),
                                    });

                                },
                                Some(Action::Connect { addr }) => {
                                    let username = {
                                        let guard = handler_state.lock().unwrap();
                                        guard.username.clone()
                                    };

                                    if username.is_empty() {
                                        let mut handler_state = handler_state.lock().unwrap();

                                        handler_state.push_notification(TextType::Error {
                                            text: String::from("[-] Must set a name"),
                                        });
                                        state_handler.updated();
                                        continue;
                                    }

                                    else {
                                        match registering_on_server(&addr, Arc::clone(&handler_state), &mut connection_handle).await {
                                            Ok(()) => {},
                                            Err(e) => {
                                                let mut handler_state = handler_state.lock().unwrap();

                                                handler_state.push_notification(TextType::Error {
                                                    text: format!("[-] Failed to register on server: {e}"),
                                                });
                                            },
                                        }
                                    }

                                },
                                Some(Action::Quit) => {
                                    let mut handler_state = handler_state.lock().unwrap();
                                    exit = true;
                                    handler_state.exit();
                                },
                                Some(Action::Invalid) => {
                                    let mut handler_state = handler_state.lock().unwrap();

                                    handler_state.push_notification(TextType::Error {
                                        text: String::from("[-] Invalid command"),
                                    });
                                },
                                _ => {
                                    let mut handler_state = handler_state.lock().unwrap();

                                    handler_state.push_notification(TextType::Error {
                                        text: String::from("[-] Not connected to a server")
                                    });
                                }
                            }

                            state_handler.updated();
                        },
                        _ = shutdown_state.recv() => {
                                break;
                        }
                    }
                }
            }
        }
    });

    // TUI Handler
    let tui_task = tokio::spawn({
        let tui_state = Arc::clone(&master_state);

        async move {
            let mut terminal = Tui::setup_terminal();

            // Get initial State
            let mut app_router = AppRouter::new(&tui_state.lock().unwrap(), tui.action_tx);
            let _ = terminal.draw(|f| app_router.render(f, ()));

            let mut ticker = tokio::time::interval(Duration::from_millis(250));

            // Main loop with three sources of events:
            // * Terminal usesr interface events such as keyboard
            // * State update channel
            // * Client shutdown channel
            loop {
                tokio::select! {
                    _ = ticker.tick() => {},
                    event = tui_events.next() => {
                        match event {
                            Ok(Event::Key(key)) => {
                                app_router.handle_key_event(key);
                            },
                            Ok(Event::Tick) => {},
                            Ok(Event::Error) => {},
                            _ => {},
                        }
                    },
                    _ = state_rx.recv() => {
                        let tui_state = tui_state.lock().unwrap();

                        app_router = app_router.update(&tui_state);
                    },
                    _ = shutdown_tui.recv() => {
                        break;
                    }
                }

                let _ = terminal.draw(|f| app_router.render(f, ()));
            }

            Tui::teardown_terminal(&mut terminal);
        }
    });

    // Wait for both tasks to finish
    let (_, _) = tokio::join!(tui_task, state_task);

    Ok(())
}

fn shutdown(_master_state: Arc<Mutex<ClientState>>) {
    println!("shutting down client");
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create broadcast channel to send shutdown signal to the different components
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<Terminate>(1);
    let mut shutdown_main = shutdown_rx.resubscribe();

    let master_state = Arc::new(Mutex::new(ClientState::default()));

    tokio::spawn({
        let master_state_clone = Arc::clone(&master_state);

        async move {
            let _ = run(
                shutdown_tx,
                &mut shutdown_rx,
                Arc::clone(&master_state_clone),
            )
            .await;
        }
    })
    .await?;

    tokio::select! {
        _ = shutdown_main.recv() => {
            shutdown(master_state);
        }
    }

    Ok(())
}
