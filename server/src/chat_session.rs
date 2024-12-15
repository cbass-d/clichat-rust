use anyhow::Result;
use std::collections::HashMap;
use tokio::{
    sync::mpsc::{self},
    task::{AbortHandle, JoinSet},
};

use super::room::UserHandle;

pub struct ChatSession {
    name: String,
    id: u64,
    pub rooms: HashMap<String, (UserHandle, AbortHandle)>,
    pub room_task_set: JoinSet<()>,
    pub mpsc_tx: mpsc::UnboundedSender<String>,
}

impl ChatSession {
    pub fn new(id: u64) -> (ChatSession, mpsc::UnboundedReceiver<String>) {
        let (mpsc_tx, mpsc_rx) = mpsc::unbounded_channel();

        (
            ChatSession {
                rooms: HashMap::new(),
                name: String::new(),
                id,
                room_task_set: JoinSet::new(),
                mpsc_tx,
            },
            mpsc_rx,
        )
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    pub fn leave_room(&mut self, room: String) -> Result<()> {
        let (_, room_task) = self.rooms.get(&room).unwrap();
        room_task.abort();

        let _ = self.rooms.remove(&room);

        Ok(())
    }
}
