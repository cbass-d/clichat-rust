use std::collections::HashSet;
use tokio::sync::broadcast::{self};

#[derive(Clone)]
pub struct UserHandle {
    room: String,
    name: String,
    broadcast_tx: broadcast::Sender<String>,
}

impl UserHandle {
    pub fn new(room: String, name: String, broadcast_tx: broadcast::Sender<String>) -> Self {
        UserHandle {
            room,
            name,
            broadcast_tx,
        }
    }

    pub fn room(&self) -> &str {
        self.room.as_ref()
    }

    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub fn send_message(&self, message: String) -> bool {
        match self.broadcast_tx.send(message) {
            Ok(..) => true,
            Err(..) => false,
        }
    }
}

pub struct Room {
    pub name: String,
    pub broadcast_tx: broadcast::Sender<String>,
    users: HashSet<String>,
}

impl Room {
    pub fn new(name: &str) -> Self {
        let (broadcast_tx, _) = broadcast::channel::<String>(10);

        Room {
            name: name.to_owned(),
            broadcast_tx,
            users: HashSet::new(),
        }
    }

    pub fn join(&mut self, user: String) -> (broadcast::Receiver<String>, UserHandle) {
        let broadcast_tx = self.broadcast_tx.clone();
        let broadcast_rx = self.broadcast_tx.subscribe();

        let user_handle = UserHandle::new(self.name.clone(), user, broadcast_tx);

        (broadcast_rx, user_handle)
    }

    pub fn leave(&mut self, user_handle: UserHandle) {
        self.users.remove(user_handle.name());
    }
}
