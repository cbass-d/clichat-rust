use anyhow::{anyhow, Result};
use std::collections::HashSet;
use tokio::sync::broadcast::{self};

#[derive(Clone, Debug)]
pub struct UserHandle {
    broadcast_tx: broadcast::Sender<String>,
}

impl UserHandle {
    pub fn new(broadcast_tx: broadcast::Sender<String>) -> Self {
        UserHandle { broadcast_tx }
    }

    pub fn send_message(&self, message: String) -> Result<()> {
        match self.broadcast_tx.send(message) {
            Ok(..) => Ok(()),
            Err(..) => Err(anyhow!("Failed to send message to room broadcast channel")),
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

        let user_handle = UserHandle::new(broadcast_tx);
        self.users.insert(user);

        (broadcast_rx, user_handle)
    }

    pub fn leave(&mut self, user: String) {
        self.users.remove(&user);
    }
}
