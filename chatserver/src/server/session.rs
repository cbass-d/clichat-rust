use anyhow::Result;
use std::collections::HashMap;
use tokio::{
    sync::mpsc::{self},
    task::{AbortHandle, JoinSet},
};

use crate::room::room_manager::RoomManager;
use crate::room::UserHandle;
use common::message::Message;

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
