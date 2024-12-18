use anyhow::Result;
use std::collections::HashMap;
use tokio::{
    sync::mpsc::{self},
    task::{AbortHandle, JoinSet},
};

use super::room::UserHandle;

pub struct ChatSession {
    pub rooms: HashMap<String, (UserHandle, AbortHandle)>,
    pub room_task_set: JoinSet<()>, // Threads for receivng room messages
    pub mpsc_tx: mpsc::UnboundedSender<String>,
}

impl ChatSession {
    pub fn new() -> (ChatSession, mpsc::UnboundedReceiver<String>) {
        let (mpsc_tx, mpsc_rx) = mpsc::unbounded_channel();

        (
            ChatSession {
                rooms: HashMap::new(),
                room_task_set: JoinSet::new(),
                mpsc_tx,
            },
            mpsc_rx,
        )
    }

    pub fn leave_room(&mut self, room: String) -> Result<()> {
        let (_, room_task) = self.rooms.get(&room).unwrap();
        room_task.abort();

        let _ = self.rooms.remove(&room);

        Ok(())
    }
}
