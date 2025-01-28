pub mod room_manager;

use anyhow::{anyhow, Result};
use common::message::Message;
use tokio::sync::broadcast::{self};

pub struct Room {
    pub name: String,
    broadcast_tx: broadcast::Sender<Message>,
}

impl Room {
    pub fn new(name: &str) -> Self {
        let (broadcast_tx, _) = broadcast::channel::<Message>(10);

        Room {
            name: name.to_owned(),
            broadcast_tx,
        }
    }

    pub fn join(&mut self) -> (broadcast::Receiver<Message>, UserHandle) {
        let broadcast_tx = self.broadcast_tx.clone();
        let broadcast_rx = self.broadcast_tx.subscribe();

        let user_handle = UserHandle::new(broadcast_tx);

        (broadcast_rx, user_handle)
    }
}
#[derive(Clone, Debug)]
pub struct UserHandle {
    broadcast_tx: broadcast::Sender<Message>,
}

impl UserHandle {
    pub fn new(broadcast_tx: broadcast::Sender<Message>) -> Self {
        UserHandle { broadcast_tx }
    }

    pub fn send_message(&self, message: Message) -> Result<()> {
        match self.broadcast_tx.send(message) {
            Ok(..) => Ok(()),
            Err(..) => Err(anyhow!("Failed to send message to room broadcast channel")),
        }
    }
}
