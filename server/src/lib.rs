pub mod chat_session;
pub mod room;

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use tokio::{
    sync::mpsc::{self},
    task::{AbortHandle, JoinSet},
};

use chat_session::ChatSession;
use room::room_manager::RoomManager;

type ClientHandle = mpsc::UnboundedSender<ServerResponse>;

#[derive(Clone)]
pub enum ClientEnd {
    ClientClosed,
    SocketError,
    ServerClose,
}

pub struct Client {
    pub id: u64,
    pub username: String,
    pub handle: ClientHandle,
    pub session: ChatSession,
}

impl Client {
    pub fn new(
        id: u64,
        session: ChatSession,
        response_tx: mpsc::UnboundedSender<ServerResponse>,
    ) -> Client {
        let handle: ClientHandle = response_tx;

        Client {
            id,
            username: String::new(),
            handle,
            session,
        }
    }
    pub async fn join_room(&mut self, room: String, room_manager: &RoomManager) {
        let session = &mut self.session;
        if session.rooms.contains_key(&room) {
            let _ = self.handle.send(ServerResponse::Failed {
                error: ServerError::AlreadyJoinedRoom.to_string(),
            });

            return;
        }

        match room_manager.join(&room, &self.username).await {
            Some((mut broadcast_rx, handle)) => {
                let room_task = session.room_task_set.spawn({
                    let mpsc_tx = session.mpsc_tx.clone();

                    async move {
                        while let Ok(message) = broadcast_rx.recv().await {
                            let _ = mpsc_tx.send(message);
                        }
                    }
                });

                session.rooms.insert(room.clone(), (handle, room_task));

                let _ = self.handle.send(ServerResponse::Joined { room });
            }

            None => {
                let _ = self.handle.send(ServerResponse::Failed {
                    error: ServerError::FailedToJoinRoom.to_string(),
                });
            }
        }
    }

    pub fn leave_room(&mut self, room: String, room_manager: &RoomManager) {
        let session = &mut self.session;

        if !session.rooms.contains_key(&room) {
            let _ = self.handle.send(ServerResponse::Failed {
                error: ServerError::NotPartOfRoom.to_string(),
            });

            return;
        }

        let room_name = room;
        let room = room_manager.rooms.get(&room_name).unwrap();
        let mut room = room.lock().unwrap();
        room.leave(self.username.clone());

        let _ = session.leave_room(room_name.clone());

        let _ = self
            .handle
            .send(ServerResponse::LeftRoom { room: room_name });
    }
}

pub struct ServerState {
    next_client_id: u64,
    pub clients: HashMap<u64, Client>,
    username_to_id: HashMap<String, u64>,
    abort_handles: HashMap<u64, AbortHandle>,
    pub connections: JoinSet<Result<ClientEnd>>,
}

impl Default for ServerState {
    fn default() -> ServerState {
        ServerState {
            next_client_id: 1,
            clients: HashMap::new(),
            username_to_id: HashMap::new(),
            abort_handles: HashMap::new(),
            connections: JoinSet::new(),
        }
    }
}

impl ServerState {
    pub fn get_next_id(&self) -> u64 {
        self.next_client_id
    }

    pub fn increment_id(&mut self) {
        self.next_client_id += 1;
    }

    pub fn add_new_client(&mut self, client: Client, abort_handle: AbortHandle) {
        let id = client.id;
        self.clients.insert(id, client);
        self.abort_handles.insert(id, abort_handle);
    }

    pub fn register(&mut self, id: u64, username: String) -> Result<()> {
        if let Some(client) = self.clients.get(&id) {
            if self.username_to_id.contains_key(&username) {
                let _ = client.handle.send(ServerResponse::Failed {
                    error: ServerError::UserNameTaken.to_string(),
                });
            } else {
                self.username_to_id.insert(username.clone(), id);
                let client = self.clients.get(&id).unwrap();
                let _ = client.handle.send(ServerResponse::Registered { username });
            }

            Ok(())
        } else {
            Err(anyhow!(ServerError::ClientNotFound))
        }
    }

    pub fn list_users(&self) -> String {
        let users: String = self
            .username_to_id
            .clone()
            .into_keys()
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
            .join(",");

        users
    }

    pub fn get_user_id(&self, username: &str) -> Option<u64> {
        self.username_to_id.get(username).copied()
    }

    pub fn change_username(&mut self, id: u64, new_username: String) -> Result<()> {
        if let Some(client) = self.clients.get(&id) {
            if self.username_to_id.contains_key(&new_username) {
                let _ = client.handle.send(ServerResponse::Failed {
                    error: ServerError::UserNameTaken.to_string(),
                });

                return Err(anyhow!(ServerError::UserNameTaken));
            } else {
                let old_username = client.username.clone();
                self.username_to_id.remove(&old_username);
                self.username_to_id.insert(new_username.clone(), id);
                let _ = client.handle.send(ServerResponse::NameChanged {
                    new_username,
                    old_username,
                });
            }
        }

        Ok(())
    }

    pub fn drop_client(&mut self, id: u64) {
        let abort_handle = self.abort_handles.get(&id).unwrap();
        abort_handle.abort();

        let client = self.clients.get(&id).unwrap();

        self.username_to_id.remove(&client.username);
        self.clients.remove(&id);
    }
}

#[derive(Clone)]
pub enum ServerRequest {
    Register {
        id: u64,
        username: String,
    },
    JoinRoom {
        room: String,
        id: u64,
    },
    LeaveRoom {
        room: String,
        id: u64,
    },
    SendTo {
        room: String,
        content: String,
        id: u64,
    },
    List {
        opt: String,
        id: u64,
    },
    DropSession {
        id: u64,
    },
    CreateRoom {
        room: String,
        id: u64,
    },
    PrivMsg {
        username: String,
        content: String,
        id: u64,
    },
    ChangeName {
        new_username: String,
        id: u64,
    },
}

pub enum ServerResponse {
    Registered {
        username: String,
    },
    Joined {
        room: String,
    },
    Listing {
        opt: String,
        content: String,
    },
    LeftRoom {
        room: String,
    },
    CreatedRoom {
        room: String,
    },
    Messaged {
        username: String,
        content: String,
    },
    NameChanged {
        new_username: String,
        old_username: String,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug)]
pub enum ServerError {
    FailedToStart,
    ClientNotFound,
    UserNameTaken,
    AlreadyJoinedRoom,
    FailedToJoinRoom,
    RoomAlreadyExists,
    NotPartOfRoom,
    UserNotFound,
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedToStart => write!(f, "Server failed to start"),
            Self::ClientNotFound => write!(f, "Client not found"),
            Self::UserNameTaken => write!(f, "Username is already in use"),
            Self::AlreadyJoinedRoom => write!(f, "Already part of room"),
            Self::FailedToJoinRoom => write!(f, "Could not join room"),
            Self::RoomAlreadyExists => write!(f, "Room already exists"),
            Self::NotPartOfRoom => write!(f, "You are not part of room"),
            Self::UserNotFound => write!(f, "User not found in server"),
        }
    }
}

impl std::error::Error for ServerError {}
