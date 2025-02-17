mod server_events;
mod session;

use common::message::{Message, MessageType};
use server_events::{ServerEvent, ServerReply};
pub use session::Session;

use crate::room::{room_manager::RoomManager, Room};
use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{
        broadcast::{self},
        mpsc::{self},
        oneshot::{self},
    },
    task::JoinSet,
};

type SessionHandle = mpsc::UnboundedSender<Message>;

pub struct Server {
    next_client_id: AtomicU64,
    port: u64,
    username_to_id: HashMap<String, u64>,
    id_to_username: HashMap<u64, String>,
    room_manager: RoomManager,
    to_server_tx: mpsc::UnboundedSender<(ServerEvent, oneshot::Sender<ServerReply>)>,
    rx: mpsc::UnboundedReceiver<(ServerEvent, oneshot::Sender<ServerReply>)>,
    sessions: HashMap<u64, (Session, SessionHandle)>,
    session_tasks: JoinSet<()>,
}

impl Server {
    pub fn new(port: u64) -> Self {
        let default_rooms: Vec<Arc<Mutex<Room>>> = vec![Arc::new(Mutex::new(Room::new("main")))];
        let room_manager = RoomManager::new(default_rooms);
        let (to_server_tx, rx) =
            mpsc::unbounded_channel::<(ServerEvent, oneshot::Sender<ServerReply>)>();

        Self {
            next_client_id: AtomicU64::new(1),
            port,
            username_to_id: HashMap::new(),
            id_to_username: HashMap::new(),
            room_manager,
            to_server_tx,
            rx,
            sessions: HashMap::new(),
            session_tasks: JoinSet::new(),
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(addr).await?;
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        info!("[+] Server started");
        info!("[+] Listening at port {0}", self.port);

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    let _ = shutdown_tx.send(());
                    break;
                },
                result = listener.accept() => match result {
                    Ok((stream, _)) => {
                        info!("[*] New connection");

                        // Spawn new thread in join set
                        self.session_tasks.spawn({
                            let to_server_tx = self.to_server_tx.clone();

                            let id = self.next_client_id.fetch_add(1, Ordering::Relaxed);
                            let (to_session_tx, session_rx, session) = Session::new(id);
                            let session_shutdown_rx = shutdown_rx.resubscribe();

                            self.sessions.insert(id, (session, to_session_tx));

                            async move {
                                let _ = handle_connection(
                                        id,
                                        stream,
                                        to_server_tx,
                                        session_rx,
                                        session_shutdown_rx
                                    ).await;
                            }
                        });
                    },
                    Err(e) => {
                        error!("[-] Failed to accept new connection: {}", e);
                    }
                },
                server_request = self.rx.recv() => {
                    if let Some((event, reply_tx)) = server_request {
                        let _ = server_events::handle_event(event, reply_tx, self).await;
                    }
                },
            }
        }

        Ok(())
    }

    pub async fn close_server(self) {
        info!("[*] Closing server");

        self.session_tasks.join_all().await;
    }
}

async fn handle_connection(
    session_id: u64,
    stream: TcpStream,
    to_server_tx: mpsc::UnboundedSender<(ServerEvent, oneshot::Sender<ServerReply>)>,
    mut session_rx: mpsc::UnboundedReceiver<Message>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<()> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Unable to accept websocket");

    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => break,
            session_message = session_rx.recv() => {
                if let Some(message) = session_message {
                    let message_bytes = message.to_bytes();

                    let _ = ws_stream.send(message_bytes.into()).await;
                }
            },
            message = ws_stream.next() => {
                match message {
                    Some(message) if message.is_ok() => {
                        let message = message.unwrap();
                        let message = Message::from_bytes(message.into_data().into());

                        if message.is_ok() {
                            let message = message.unwrap();
                            let reply_message = handle_message(message, session_id, to_server_tx.clone()).await;

                            match reply_message {
                                Ok(message) => {
                                    let _ = ws_stream.send(message.to_bytes().into()).await;
                                },
                                Err(_) => {},
                            }
                        }
                    },
                    None => {
                        // Connection to the client has been closed/dropped
                        // session has to be dropped as well
                        let event = ServerEvent::DropSession {
                            id: session_id,
                        };

                        let (tx, _rx) = oneshot::channel::<ServerReply>();
                        let _ = to_server_tx.send((event, tx));

                        break;
                    },
                    _ => {},
                }
            },
        }
    }

    Ok(())
}

async fn handle_message(
    message: Message,
    session_id: u64,
    to_server_tx: mpsc::UnboundedSender<(ServerEvent, oneshot::Sender<ServerReply>)>,
) -> Result<Message> {
    let header = message.header;
    match header.message_type {
        MessageType::Register => {
            let body = message.body;
            let event = ServerEvent::Register {
                id: session_id,
                username: body.arg.unwrap(),
            };

            let (tx, rx) = oneshot::channel::<ServerReply>();
            let _ = to_server_tx.send((event, tx));

            // Get reply from sever
            let server_reply = rx.await;

            match server_reply {
                Ok(ServerReply::Registered { username }) => {
                    let message = Message::build(
                        MessageType::Registered,
                        0,
                        Some(session_id.to_string()),
                        Some(username),
                    )?;

                    Ok(message)
                }
                Ok(ServerReply::Failed { error }) => {
                    let message = Message::build(
                        MessageType::Failed,
                        0,
                        Some(String::from("register")),
                        Some(error),
                    )?;

                    Ok(message)
                }
                _ => Err(anyhow!("Unexpected server reply")),
            }
        }
        MessageType::ChangeName => {
            let body = message.body;
            let event = ServerEvent::ChangeName {
                id: session_id,
                new_username: body.arg.unwrap(),
            };

            let (tx, rx) = oneshot::channel::<ServerReply>();
            let _ = to_server_tx.send((event, tx));

            let server_reply = rx.await;

            match server_reply {
                Ok(ServerReply::NameChanged {
                    new_username,
                    old_username,
                }) => {
                    let message = Message::build(
                        MessageType::ChangedName,
                        0,
                        Some(new_username),
                        Some(old_username),
                    )?;

                    Ok(message)
                }
                Ok(ServerReply::Failed { error }) => {
                    let message = Message::build(
                        MessageType::Failed,
                        0,
                        Some(String::from("changename")),
                        Some(error),
                    )?;

                    Ok(message)
                }
                _ => Err(anyhow!("Unexpected server reply")),
            }
        }
        MessageType::Join => {
            let body = message.body;
            let room = body.arg.unwrap();

            let event = ServerEvent::JoinRoom {
                id: session_id,
                room,
            };

            let (tx, rx) = oneshot::channel::<ServerReply>();
            let _ = to_server_tx.send((event, tx));

            let server_reply = rx.await;

            match server_reply {
                Ok(ServerReply::Joined { room }) => {
                    let message = Message::build(MessageType::Joined, 0, Some(room), None)?;

                    Ok(message)
                }
                Ok(ServerReply::Failed { error }) => {
                    let message = Message::build(
                        MessageType::Failed,
                        0,
                        Some(String::from("join")),
                        Some(error),
                    )?;

                    Ok(message)
                }
                _ => Err(anyhow!("Unexpected server reply")),
            }
        }
        MessageType::Leave => {
            let body = message.body;
            let room = body.arg.unwrap();

            let event = ServerEvent::LeaveRoom {
                id: session_id,
                room,
            };

            let (tx, rx) = oneshot::channel::<ServerReply>();
            let _ = to_server_tx.send((event, tx));

            let server_reply = rx.await;

            match server_reply {
                Ok(ServerReply::LeftRoom { room }) => {
                    let message = Message::build(MessageType::LeftRoom, 0, Some(room), None)?;

                    Ok(message)
                }
                Ok(ServerReply::Failed { error }) => {
                    let message = Message::build(
                        MessageType::Failed,
                        0,
                        Some(String::from("leave")),
                        Some(error),
                    )?;

                    Ok(message)
                }
                _ => Err(anyhow!("Unexpected server reply")),
            }
        }
        MessageType::Create => {
            let body = message.body;
            let room = body.arg.unwrap();

            let event = ServerEvent::CreateRoom { room };

            let (tx, rx) = oneshot::channel::<ServerReply>();
            let _ = to_server_tx.send((event, tx));

            let server_reply = rx.await;

            match server_reply {
                Ok(ServerReply::CreatedRoom { room }) => {
                    let message = Message::build(MessageType::CreatedRoom, 0, Some(room), None)?;

                    Ok(message)
                }
                Ok(ServerReply::Failed { error }) => {
                    let message = Message::build(
                        MessageType::Failed,
                        0,
                        Some(String::from("create")),
                        Some(error),
                    )?;

                    Ok(message)
                }
                _ => Err(anyhow!("Unexpected server reply")),
            }
        }
        MessageType::SendTo => {
            let body = message.body;
            let room = body.arg.unwrap();
            let content = body.content.unwrap();
            let event = ServerEvent::SendTo {
                id: session_id,
                room: room.clone(),
                content: content.clone(),
            };

            let (tx, rx) = oneshot::channel::<ServerReply>();
            let _ = to_server_tx.send((event, tx));

            let server_reply = rx.await;

            match server_reply {
                Ok(ServerReply::MessagedRoom) => {
                    let message =
                        Message::build(MessageType::MessagedRoom, 0, Some(room), Some(content))?;

                    Ok(message)
                }
                Ok(ServerReply::Failed { error }) => {
                    let message = Message::build(
                        MessageType::Failed,
                        0,
                        Some(String::from("sendto")),
                        Some(error),
                    )?;

                    Ok(message)
                }
                _ => Err(anyhow!("Unexpected server reply")),
            }
        }
        MessageType::List => {
            let body = message.body;
            let opt = body.arg.unwrap();

            let event = ServerEvent::List {
                id: session_id,
                opt,
            };

            let (tx, rx) = oneshot::channel::<ServerReply>();
            let _ = to_server_tx.send((event, tx));

            let server_reply = rx.await;

            match server_reply {
                Ok(ServerReply::ListingUsers { content }) => {
                    let message = Message::build(MessageType::Users, 0, None, Some(content))?;

                    Ok(message)
                }
                Ok(ServerReply::ListingUserRooms { content }) => {
                    let message = Message::build(MessageType::UserRooms, 0, None, Some(content))?;

                    Ok(message)
                }
                Ok(ServerReply::ListingRooms { content }) => {
                    let message = Message::build(MessageType::AllRooms, 0, None, Some(content))?;

                    Ok(message)
                }
                Ok(ServerReply::Failed { error }) => {
                    let message = Message::build(
                        MessageType::Failed,
                        0,
                        Some(String::from("list")),
                        Some(error),
                    )?;

                    Ok(message)
                }
                _ => Err(anyhow!("Unexpected server reply")),
            }
        }
        MessageType::PrivMsg => {
            let body = message.body;
            let username = body.arg.unwrap();
            let content = body.content.unwrap();
            let event = ServerEvent::PrivMsg {
                id: session_id,
                username: username.clone(),
                content: content.clone(),
            };

            let (tx, rx) = oneshot::channel::<ServerReply>();
            let _ = to_server_tx.send((event, tx));

            let server_reply = rx.await;

            match server_reply {
                Ok(ServerReply::MessagedUser) => {
                    let message =
                        Message::build(MessageType::OutgoingMsg, 0, Some(username), Some(content))?;

                    Ok(message)
                }
                Ok(ServerReply::Failed { error }) => {
                    let message = Message::build(
                        MessageType::Failed,
                        0,
                        Some(String::from("privmsg")),
                        Some(error),
                    )?;

                    Ok(message)
                }
                _ => Err(anyhow!("Unexpected server reply")),
            }
        }
        _ => Err(anyhow!("Unexpected message type")),
    }
}
