mod client_connection;
mod session;

pub use client_connection::ClientConnection;
use common::message::{Message, MessageBody, MessageHeader, MessageType};
pub use session::Session;

use crate::room::{room_manager::RoomManager, Room};
use anyhow::Result;
use log::{error, info};
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use tokio::{
    net::TcpListener,
    sync::{
        broadcast::{self},
        mpsc::{self},
        oneshot::{self},
    },
    task::JoinSet,
};

#[derive(Clone)]
pub enum ServerEvent {
    Register {
        id: u64,
        username: String,
    },
    JoinRoom {
        id: u64,
        room: String,
    },
    LeaveRoom {
        id: u64,
        room: String,
    },
    SendTo {
        id: u64,
        room: String,
        content: String,
    },
    List {
        id: u64,
        opt: String,
    },
    DropSession {
        id: u64,
    },
    CreateRoom {
        room: String,
    },
    PrivMsg {
        id: u64,
        username: String,
        content: String,
    },
    ChangeName {
        id: u64,
        new_username: String,
    },
}

#[derive(Clone)]
pub enum ServerReply {
    Registered {
        username: String,
    },
    Joined {
        room: String,
    },
    ListingUsers {
        content: String,
    },
    ListingRooms {
        content: String,
    },
    ListingUserRooms {
        content: String,
    },
    LeftRoom {
        room: String,
    },
    CreatedRoom {
        room: String,
    },
    MessagedUser,
    MessagedRoom,
    NameChanged {
        new_username: String,
        old_username: String,
    },
    Failed {
        error: String,
    },
}

type SessionHandle = mpsc::UnboundedSender<Message>;

pub struct Server {
    next_client_id: u64,
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
            next_client_id: 1,
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

                            let id = self.next_client_id;
                            self.next_client_id += 1;
                            let client_connection = ClientConnection::new(stream);
                            let (to_session_tx, session_rx, session) = Session::new();
                            let session_shutdown_rx = shutdown_rx.resubscribe();

                            self.sessions.insert(id, (session, to_session_tx));

                            async move {
                                handle_session(
                                        id,
                                        client_connection,
                                        to_server_tx,
                                        session_rx,
                                        session_shutdown_rx
                                    ).await;
                            }
                        });
                    }
                    Err(e) => {
                        error!("[-] Failed to accept new connection: {}", e);
                    }
                },
                server_request = self.rx.recv() => {
                    let (event, reply_tx) = server_request.unwrap();
                    match event {
                        ServerEvent::Register { id, username } => {

                            if self.username_to_id.contains_key(&username) {
                                let reply = ServerReply::Failed {
                                    error: String::from("Username already exists"),
                                };

                                let _ = reply_tx.send(reply);
                            }

                            else {
                                let (session, _) = self.sessions.get_mut(&id).unwrap();
                                session.set_username(&username);
                                self.username_to_id.insert(username.clone(), id);
                                self.id_to_username.insert(id, username.clone());
                                let reply = ServerReply::Registered { username };

                                let _ = reply_tx.send(reply);
                            }
                        },
                        ServerEvent::ChangeName { id, new_username } => {
                            if self.username_to_id.contains_key(&new_username) {
                                let reply = ServerReply::Failed {
                                    error: String::from("Username already exists"),
                                };

                                let _ = reply_tx.send(reply);
                            }

                            else {
                                let (session, _) = self.sessions.get_mut(&id).unwrap();
                                session.set_username(&new_username);
                                let old_username = self.id_to_username[&id].clone();
                                *self.id_to_username.get_mut(&id).unwrap() = new_username.clone();
                                self.username_to_id.remove(&old_username);
                                self.username_to_id.insert(new_username.clone(), id);

                                let reply = ServerReply::NameChanged {
                                    new_username,
                                    old_username,
                                };

                                let _ = reply_tx.send(reply);
                            }
                        },
                        ServerEvent::List { id, opt } => {
                            match opt.as_ref() {
                                "users" => {
                                    let users = self.username_to_id.clone().into_keys().map(|s| s.to_string())
                                        .collect::<Vec<String>>()
                                        .join(",");

                                    let reply = ServerReply::ListingUsers {
                                        content: users,
                                    };

                                    let _ = reply_tx.send(reply);
                                },
                                "rooms" => {
                                    let (session, _) = self.sessions.get_mut(&id).unwrap();
                                    let user_rooms = session.rooms.clone().into_keys()
                                        .map(|s| s.to_string())
                                        .collect::<Vec<String>>()
                                        .join(",");

                                    let reply = ServerReply::ListingUserRooms {
                                        content: user_rooms,
                                    };

                                    let _ = reply_tx.send(reply);
                                },
                                "allrooms" => {
                                    let rooms = self.room_manager.get_rooms();
                                    let rooms = rooms.into_iter().map(|s| s.to_string())
                                        .collect::<Vec<String>>()
                                        .join(",");

                                    let reply = ServerReply::ListingRooms {
                                        content: rooms,
                                    };

                                    let _ = reply_tx.send(reply);
                                },
                                _ => {
                                    let reply = ServerReply::Failed {
                                        error: String::from("Invalid option")
                                    };

                                    let _ = reply_tx.send(reply);
                                },
                            }
                        },
                        ServerEvent::JoinRoom { id, room } => {
                            let (session, _) = self.sessions.get_mut(&id).unwrap();

                            if session.rooms.contains_key(&room) {
                                let reply = ServerReply::Failed {
                                    error: String::from("Already part of room"),
                                };

                                let _ = reply_tx.send(reply);
                            }

                            else {
                                match session.join_room(&room, &self.room_manager).await {
                                    Ok(()) => {
                                        let reply = ServerReply::Joined {
                                            room: room,
                                        };

                                        let _ = reply_tx.send(reply);
                                    },
                                    Err(e) => {
                                        let reply = ServerReply::Failed {
                                            error: e.to_string(),
                                        };

                                        let _ = reply_tx.send(reply);
                                    },
                                }
                            }
                        },
                        ServerEvent::LeaveRoom { id, room } => {
                            let (session, _) = self.sessions.get_mut(&id).unwrap();

                            if !session.rooms.contains_key(&room) {
                                let reply = ServerReply::Failed {
                                    error: String::from("Not part of room"),
                                };

                                let _ = reply_tx.send(reply);
                            }

                            else {
                                match session.leave_room(&room) {
                                    Ok(()) => {
                                        let reply = ServerReply::LeftRoom{
                                            room: room,
                                        };

                                        let _ = reply_tx.send(reply);
                                    },
                                    Err(e) => {
                                        let reply = ServerReply::Failed {
                                            error: e.to_string(),
                                        };

                                        let _ = reply_tx.send(reply);
                                    },
                                }
                            }
                        },
                        ServerEvent::CreateRoom { room } => {
                            let existing_rooms = self.room_manager.get_rooms();

                            if existing_rooms.contains(&room) {
                                let reply = ServerReply::Failed {
                                    error: String::from("Room already exists"),
                                };

                                let _ = reply_tx.send(reply);
                            }

                            else {
                                let new_room = Arc::new(Mutex::new(Room::new(&room)));
                                self.room_manager.add_room(new_room, room.clone());

                                let reply = ServerReply::CreatedRoom {
                                    room,
                                };

                                let _ = reply_tx.send(reply);
                            }

                        },
                        ServerEvent::SendTo { id, room, content } => {
                            let (session, _) = self.sessions.get_mut(&id).unwrap();

                            if let Some((room_handle, _)) = session.rooms.get(&room) {
                                let username = &session.username;

                                let header = MessageHeader {
                                    message_type: MessageType::RoomMessage,
                                    sender_id: id,
                                };

                                let body = MessageBody {
                                    arg: Some(room.clone()),
                                    content: Some(format!("{username}: {content}")),
                                };

                                let message = Message {
                                    header,
                                    body,
                                };

                                let _ = room_handle.send_message(message);

                                let reply = ServerReply::MessagedRoom;

                                let _ = reply_tx.send(reply);
                            }

                            else {
                                let reply = ServerReply::Failed {
                                    error: String::from("Not part of room"),
                                };

                                let _ = reply_tx.send(reply);
                            }
                        },
                        ServerEvent::PrivMsg { id, username, content } => {

                            if !self.username_to_id.contains_key(&username) {
                                let reply = ServerReply::Failed {
                                    error: String::from("No such user"),
                                };

                                let _ = reply_tx.send(reply);
                            }

                            else {
                                let sender = &self.id_to_username[&id];
                                let receiver_id = self.username_to_id[&username];
                                let (_, receiving_session_tx) = self.sessions.get_mut(&receiver_id).unwrap();

                                let header = MessageHeader {
                                    sender_id: id,
                                    message_type: MessageType::IncomingMsg,
                                };

                                let body = MessageBody {
                                    arg: None,
                                    content: Some(format!("from {sender}: {content}")),
                                };

                                let message = Message {
                                    header,
                                    body,
                                };

                                let _ = receiving_session_tx.send(message);

                                let reply = ServerReply::MessagedUser;

                                let _ = reply_tx.send(reply);
                            }

                        },
                        ServerEvent::DropSession { id } => {
                            if self.id_to_username.contains_key(&id) {
                                let username = &self.id_to_username[&id].clone();
                                self.id_to_username.remove(&id);
                                self.username_to_id.remove(username);
                            }
                            self.sessions.remove(&id);
                        },
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

pub async fn handle_session(
    session_id: u64,
    mut client_connection: ClientConnection,
    to_server_tx: mpsc::UnboundedSender<(ServerEvent, oneshot::Sender<ServerReply>)>,
    mut session_rx: mpsc::UnboundedReceiver<Message>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => break,
            session_message = session_rx.recv() => {
                match session_message {
                    Some(message) => {

                        // Add small delay for sending message
                        // When server sends two messages rapidly one of the messsages gets lost on
                        // the client side
                        // TODO: Find a better solution to this problem
                        std::thread::sleep(std::time::Duration::from_millis(3));

                        let _ = client_connection.write(message).await;
                    },
                    None => {},
                }
            },
            read_result = client_connection.read() => {
                match read_result {
                    Ok(message) => {
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
                                let server_reply = rx.await.unwrap();

                                match server_reply {
                                    ServerReply::Registered { username } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Registered,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(session_id.to_string()),
                                            content: Some(username),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    ServerReply::Failed { error } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Failed,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(String::from("register")),
                                            content: Some(error),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
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

                                let server_reply = rx.await.unwrap();

                                match server_reply {
                                    ServerReply::NameChanged { new_username, old_username } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::ChangedName,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(new_username),
                                            content: Some(old_username),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    ServerReply::Failed { error } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Failed,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(String::from("changename")),
                                            content: Some(error),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
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

                                let server_reply = rx.await.unwrap();

                                match server_reply {
                                    ServerReply::Joined { room } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Joined,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(room),
                                            content: None,
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    ServerReply::Failed { error } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Failed,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(String::from("join")),
                                            content: Some(error),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
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

                                let server_reply = rx.await.unwrap();

                                match server_reply {
                                    ServerReply::LeftRoom { room } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::LeftRoom,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(room),
                                            content: None,
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    ServerReply::Failed { error } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Failed,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(String::from("leave")),
                                            content: Some(error),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
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

                                let server_reply = rx.await.unwrap();

                                match server_reply {
                                    ServerReply::CreatedRoom { room }=> {
                                        let header = MessageHeader {
                                            message_type: MessageType::CreatedRoom,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(room),
                                            content: None,
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    ServerReply::Failed { error } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Failed,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(String::from("create")),
                                            content: Some(error),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
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

                                let server_reply = rx.await.unwrap();

                                match server_reply {
                                    ServerReply::MessagedRoom => {
                                        let header = MessageHeader {
                                            message_type: MessageType::MessagedRoom,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(room),
                                            content: Some(content),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    ServerReply::Failed { error } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Failed,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(String::from("sendto")),
                                            content: Some(error),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
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

                                let server_reply = rx.await.unwrap();

                                match server_reply {
                                    ServerReply::ListingUsers { content } => {
                                        let header = MessageHeader {
                                            sender_id: 0,
                                            message_type: MessageType::Users,
                                        };
                                        let body = MessageBody {
                                            arg: None,
                                            content: Some(content),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    ServerReply::ListingUserRooms { content } => {
                                        let header = MessageHeader {
                                            sender_id: 0,
                                            message_type: MessageType::UserRooms,
                                        };
                                        let body = MessageBody {
                                            arg: None,
                                            content: Some(content),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    ServerReply::ListingRooms { content } => {
                                        let header = MessageHeader {
                                            sender_id: 0,
                                            message_type: MessageType::AllRooms,
                                        };
                                        let body = MessageBody {
                                            arg: None,
                                            content: Some(content),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    ServerReply::Failed { error } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Failed,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(String::from("list")),
                                            content: Some(error),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
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

                                let server_reply = rx.await.unwrap();

                                match server_reply {
                                    ServerReply::MessagedUser => {
                                        let header = MessageHeader {
                                            sender_id: 0,
                                            message_type: MessageType::OutgoingMsg,
                                        };
                                        let body = MessageBody {
                                            arg: Some(username),
                                            content: Some(content),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                       let _ = client_connection.write(message).await;

                                    },
                                    ServerReply::Failed { error } => {
                                        let header = MessageHeader {
                                            message_type: MessageType::Failed,
                                            sender_id: 0,
                                        };
                                        let body = MessageBody {
                                            arg: Some(String::from("privmsg")),
                                            content: Some(error),
                                        };
                                        let message = Message {
                                            header,
                                            body,
                                        };

                                        let _ = client_connection.write(message).await;
                                    },
                                    _ => {},
                                }
                            },
                            _ => {},
                        }
                    },
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {},
                    Err(_) => {

                        // Connection to the client has been closed/dropped
                        // session has to be dropped as well
                        let event = ServerEvent::DropSession {
                            id: session_id,
                        };

                        let (tx, _rx) = oneshot::channel::<ServerReply>();
                        let _ = to_server_tx.send((event, tx));

                        break;
                    },
                }
            },
        }
    }
}
