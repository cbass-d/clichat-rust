use anyhow::{anyhow, Result};
use std::collections::HashMap;
use tokio::{
    sync::mpsc::{self},
    task::{AbortHandle, JoinSet},
};

use crate::room::room_manager::RoomManager;
use crate::room::UserHandle;
use crate::server::{Message, MessageType};

pub struct Session {
    pub id: u64,
    pub username: String,
    rooms: HashMap<String, (UserHandle, AbortHandle)>,
    room_task_set: JoinSet<()>, // Threads for receivng room messages
    to_session_tx: mpsc::UnboundedSender<Message>,
}

impl Session {
    pub fn new(
        id: u64,
    ) -> (
        mpsc::UnboundedSender<Message>,
        mpsc::UnboundedReceiver<Message>,
        Self,
    ) {
        let (to_session_tx, rx) = mpsc::unbounded_channel::<Message>();

        (
            to_session_tx.clone(),
            rx,
            Self {
                id,
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
        if self.rooms.contains_key(room) {
            return Err(anyhow!("Already part of room"));
        }

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

    pub fn joined_rooms(&self) -> String {
        let rooms = self
            .rooms
            .clone()
            .into_keys()
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
            .join(",");

        rooms
    }

    pub async fn send_room_message(&self, room: &str, content: &str) -> Result<()> {
        if let Some((room_handle, _)) = self.rooms.get(room) {
            let message = Message::build(
                MessageType::RoomMessage,
                self.id,
                Some(room.to_string()),
                Some(format!("{0}: {content}", self.username)),
            )?;

            let _ = room_handle.send_message(message);

            Ok(())
        } else {
            Err(anyhow!("Not part of room"))
        }
    }

    pub fn leave_room(&mut self, room: &str) -> Result<()> {
        if !self.rooms.contains_key(room) {
            Err(anyhow!("Not part of room"))
        } else {
            let (_, room_task) = self.rooms.get(room).unwrap();
            room_task.abort();

            let _ = self.rooms.remove(room);

            Ok(())
        }
    }
}
