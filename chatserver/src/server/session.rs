use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio::{
    net::TcpStream,
    sync::{
        broadcast::{self},
        mpsc::{self},
        oneshot::{self},
    },
    task::{AbortHandle, JoinSet},
};

use crate::room::room_manager::RoomManager;
use crate::room::UserHandle;
use crate::server::{Message, MessageType, ServerEvent, ServerReply};

pub struct Session {
    pub username: String,
    pub rooms: HashMap<String, (UserHandle, AbortHandle)>,
    room_task_set: JoinSet<()>, // Threads for receivng room messages
    to_session_tx: mpsc::UnboundedSender<Message>,
}

impl Session {
    pub fn new() -> (
        mpsc::UnboundedSender<Message>,
        mpsc::UnboundedReceiver<Message>,
        Self,
    ) {
        let (to_session_tx, rx) = mpsc::unbounded_channel::<Message>();

        (
            to_session_tx.clone(),
            rx,
            Self {
                username: String::new(),
                rooms: HashMap::new(),
                room_task_set: JoinSet::new(),
                to_session_tx,
            },
        )
    }

    pub fn set_username(&mut self, username: &str) {
        self.username = username.to_string();
    }

    pub async fn join_room(&mut self, room: &str, room_manager: &RoomManager) -> Result<()> {
        let (mut broadcast_rx, handle) = room_manager.join(room).await?;

        let room_task = self.room_task_set.spawn({
            let to_session_tx = self.to_session_tx.clone();

            async move {
                while let Ok(message) = broadcast_rx.recv().await {
                    let _ = to_session_tx.send(message);
                }
            }
        });

        self.rooms.insert(room.to_string(), (handle, room_task));

        Ok(())
    }

    pub fn leave_room(&mut self, room: &str) -> Result<()> {
        let (_, room_task) = self.rooms.get(room).unwrap();
        room_task.abort();

        let _ = self.rooms.remove(room);

        Ok(())
    }
}

pub async fn handle_session(
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
                        let message = Message::from_bytes(message.into_data().into()).unwrap();

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
                                        if let Ok(message) = Message::build(
                                                MessageType::Registered,
                                                0,
                                                Some(session_id.to_string()),
                                                Some(username),
                                            ) {

                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    Ok(ServerReply::Failed { error }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::Failed,
                                                0,
                                                Some(String::from("register")),
                                                Some(error),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    _ => {},
                                }

                            },
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
                                    Ok(ServerReply::NameChanged { new_username, old_username }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::ChangedName,
                                                0,
                                                Some(new_username),
                                                Some(old_username),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }
                                    },
                                    Ok(ServerReply::Failed { error }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::Failed,
                                                0,
                                                Some(String::from("changename")),
                                                Some(error),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    _ => {},
                                }
                            },
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
                                        if let Ok(message) = Message::build(
                                                MessageType::Joined,
                                                0,
                                                Some(room),
                                                None,
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    Ok(ServerReply::Failed { error }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::Failed,
                                                0,
                                                Some(String::from("join")),
                                                Some(error),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    _ => {},
                                }

                            },
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
                                        if let Ok(message) = Message::build(
                                                MessageType::LeftRoom,
                                                0,
                                                Some(room),
                                                None,
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    Ok(ServerReply::Failed { error }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::Failed,
                                                0,
                                                Some(String::from("leave")),
                                                Some(error),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    _ => {},
                                }

                            },
                            MessageType::Create => {
                                let body = message.body;
                                let room = body.arg.unwrap();

                                let event = ServerEvent::CreateRoom {
                                    room,
                                };

                                let (tx, rx) = oneshot::channel::<ServerReply>();
                                let _ = to_server_tx.send((event, tx));

                                let server_reply = rx.await;

                                match server_reply {
                                    Ok(ServerReply::CreatedRoom { room }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::CreatedRoom,
                                                0,
                                                Some(room),
                                                None,
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    Ok(ServerReply::Failed { error }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::Failed,
                                                0,
                                                Some(String::from("create")),
                                                Some(error),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    _ => {},
                                }


                            },
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
                                        if let Ok(message) = Message::build(
                                                MessageType::MessagedRoom,
                                                0,
                                                Some(room),
                                                Some(content),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    Ok(ServerReply::Failed { error }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::Failed,
                                                0,
                                                Some(String::from("sendto")),
                                                Some(error),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    _ => {},
                                }
                            },
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
                                        if let Ok(message) = Message::build(
                                                MessageType::Users,
                                                0,
                                                None,
                                                Some(content),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    Ok(ServerReply::ListingUserRooms { content }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::UserRooms,
                                                0,
                                                None,
                                                Some(content),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    Ok(ServerReply::ListingRooms { content }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::AllRooms,
                                                0,
                                                None,
                                                Some(content),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    Ok(ServerReply::Failed { error }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::Failed,
                                                0,
                                                Some(String::from("list")),
                                                Some(error),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    _ => {},
                                }
                            },
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
                                        if let Ok(message) = Message::build(
                                                MessageType::OutgoingMsg,
                                                0,
                                                Some(username),
                                                Some(content),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    Ok(ServerReply::Failed { error }) => {
                                        if let Ok(message) = Message::build(
                                                MessageType::Failed,
                                                0,
                                                Some(String::from("privmsg")),
                                                Some(error),
                                            ) {
                                                let _ = ws_stream.send(message.to_bytes().into()).await;
                                            }

                                    },
                                    _ => {},
                                }
                            },
                            _ => {},
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
