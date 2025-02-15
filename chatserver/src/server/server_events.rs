use anyhow::Result;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot::{self};

use crate::server::{Message, MessageType, Room, Server};

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

pub async fn handle_event(
    event: ServerEvent,
    reply_tx: oneshot::Sender<ServerReply>,
    server: &mut Server,
) -> Result<()> {
    match event {
        ServerEvent::Register { id, username } => {
            if server.username_to_id.contains_key(&username) {
                let reply = ServerReply::Failed {
                    error: String::from("Username already exists"),
                };

                let _ = reply_tx.send(reply);
            } else if let Some((session, _)) = server.sessions.get_mut(&id) {
                session.set_username(&username);

                server.username_to_id.insert(username.clone(), id);
                server.id_to_username.insert(id, username.clone());

                let reply = ServerReply::Registered { username };

                let _ = reply_tx.send(reply);
            }
        }
        ServerEvent::ChangeName { id, new_username } => {
            if server.username_to_id.contains_key(&new_username) {
                let reply = ServerReply::Failed {
                    error: String::from("Username already exists"),
                };

                let _ = reply_tx.send(reply);
            } else if let Some((session, _)) = server.sessions.get_mut(&id) {
                session.set_username(&new_username);
                let old_username = server.id_to_username[&id].clone();

                *server.id_to_username.get_mut(&id).unwrap() = new_username.clone();
                server.username_to_id.remove(&old_username);
                server.username_to_id.insert(new_username.clone(), id);

                let reply = ServerReply::NameChanged {
                    new_username,
                    old_username,
                };

                let _ = reply_tx.send(reply);
            }
        }
        ServerEvent::List { id, opt } => match opt.as_ref() {
            "users" => {
                let users = server
                    .username_to_id
                    .clone()
                    .into_keys()
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
                    .join(",");

                let reply = ServerReply::ListingUsers { content: users };

                let _ = reply_tx.send(reply);
            }
            "rooms" => {
                if let Some((session, _)) = server.sessions.get_mut(&id) {
                    let user_rooms = session
                        .rooms
                        .clone()
                        .into_keys()
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>()
                        .join(",");

                    let reply = ServerReply::ListingUserRooms {
                        content: user_rooms,
                    };

                    let _ = reply_tx.send(reply);
                }
            }
            "allrooms" => {
                let rooms = server.room_manager.get_rooms();
                let rooms = rooms
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
                    .join(",");

                let reply = ServerReply::ListingRooms { content: rooms };

                let _ = reply_tx.send(reply);
            }
            _ => {
                let reply = ServerReply::Failed {
                    error: String::from("Invalid option"),
                };

                let _ = reply_tx.send(reply);
            }
        },
        ServerEvent::JoinRoom { id, room } => {
            if let Some((session, _)) = server.sessions.get_mut(&id) {
                if session.rooms.contains_key(&room) {
                    let reply = ServerReply::Failed {
                        error: String::from("Already part of room"),
                    };

                    let _ = reply_tx.send(reply);
                } else {
                    match session.join_room(&room, &server.room_manager).await {
                        Ok(()) => {
                            let reply = ServerReply::Joined { room };

                            let _ = reply_tx.send(reply);
                        }
                        Err(e) => {
                            let reply = ServerReply::Failed {
                                error: e.to_string(),
                            };

                            let _ = reply_tx.send(reply);
                        }
                    }
                }
            }
        }
        ServerEvent::LeaveRoom { id, room } => {
            if let Some((session, _)) = server.sessions.get_mut(&id) {
                if !session.rooms.contains_key(&room) {
                    let reply = ServerReply::Failed {
                        error: String::from("Not part of room"),
                    };

                    let _ = reply_tx.send(reply);
                } else {
                    match session.leave_room(&room) {
                        Ok(()) => {
                            let reply = ServerReply::LeftRoom { room };

                            let _ = reply_tx.send(reply);
                        }
                        Err(e) => {
                            let reply = ServerReply::Failed {
                                error: e.to_string(),
                            };

                            let _ = reply_tx.send(reply);
                        }
                    }
                }
            }
        }
        ServerEvent::CreateRoom { room } => {
            let existing_rooms = server.room_manager.get_rooms();

            if existing_rooms.contains(&room) {
                let reply = ServerReply::Failed {
                    error: String::from("Room already exists"),
                };

                let _ = reply_tx.send(reply);
            } else {
                let new_room = Arc::new(Mutex::new(Room::new(&room)));
                server.room_manager.add_room(new_room, room.clone());

                let reply = ServerReply::CreatedRoom { room };

                let _ = reply_tx.send(reply);
            }
        }
        ServerEvent::SendTo { id, room, content } => {
            if let Some((session, _)) = server.sessions.get_mut(&id) {
                if let Some((room_handle, _)) = session.rooms.get(&room) {
                    let username = &session.username;

                    if let Ok(message) = Message::build(
                        MessageType::RoomMessage,
                        id,
                        Some(room.clone()),
                        Some(format!("{username}: {content}")),
                    ) {
                        let _ = room_handle.send_message(message);

                        let reply = ServerReply::MessagedRoom;

                        let _ = reply_tx.send(reply);
                    }
                } else {
                    let reply = ServerReply::Failed {
                        error: String::from("Not part of room"),
                    };

                    let _ = reply_tx.send(reply);
                }
            }
        }
        ServerEvent::PrivMsg {
            id,
            username,
            content,
        } => {
            if !server.username_to_id.contains_key(&username) {
                let reply = ServerReply::Failed {
                    error: String::from("No such user"),
                };

                let _ = reply_tx.send(reply);
            } else {
                let sender = &server.id_to_username[&id];
                let receiver_id = server.username_to_id[&username];

                if let Some((_, receiving_session_tx)) = server.sessions.get_mut(&receiver_id) {
                    if let Ok(message) = Message::build(
                        MessageType::IncomingMsg,
                        id,
                        None,
                        Some(format!("from {sender}: {content}")),
                    ) {
                        let _ = receiving_session_tx.send(message);

                        let reply = ServerReply::MessagedUser;

                        let _ = reply_tx.send(reply);
                    }
                } else {
                    let reply = ServerReply::Failed {
                        error: String::from("Receiving session not found"),
                    };

                    let _ = reply_tx.send(reply);
                }
            }
        }
        ServerEvent::DropSession { id } => {
            if server.id_to_username.contains_key(&id) {
                let username = &server.id_to_username[&id].clone();
                server.id_to_username.remove(&id);
                server.username_to_id.remove(username);
            }
            server.sessions.remove(&id);
        }
    }

    Ok(())
}
