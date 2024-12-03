use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::{
    sync::mpsc::{self},
    task::{AbortHandle, JoinSet},
};

use super::{room::UserHandle, RoomManager};

pub struct ChatSession {
    name: String,
    id: u64,
    pub rooms: HashMap<String, (UserHandle, AbortHandle)>,
    room_manager: Arc<RoomManager>,
    room_task_set: JoinSet<()>,
    mpsc_tx: mpsc::Sender<String>,
    mpsc_rx: mpsc::Receiver<String>,
}

impl ChatSession {
    pub fn new(id: u64, room_manager: Arc<RoomManager>) -> Self {
        let (mpsc_tx, mpsc_rx) = mpsc::channel(10);

        Self {
            rooms: HashMap::new(),
            name: String::new(),
            id,
            room_manager,
            room_task_set: JoinSet::new(),
            mpsc_tx,
            mpsc_rx,
        }
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

    pub async fn join_room(&mut self, room: String) -> String {
        if self.rooms.contains_key(&room) {
            return format!("[-] Already part of {room}");
        }

        if let Some((mut broadcast_rx, user_handle)) =
            self.room_manager.join(room.as_ref(), &self.name).await
        {
            let room_task = self.room_task_set.spawn({
                let mpsc_tx = self.mpsc_tx.clone();

                async move {
                    while let Ok(message) = broadcast_rx.recv().await {
                        let _ = mpsc_tx.send(message).await;
                    }
                }
            });

            self.rooms.insert(room.clone(), (user_handle, room_task));

            return format!("[+] Joined {room}");
        }

        format!("[-] Failed to join {room}")
    }

    pub fn leave_room(&mut self, room: String) -> String {
        if !self.rooms.contains_key(&room) {
            return format!("[-] Not part of {room}");
        }

        let (_, room_task) = self.rooms.get(&room).unwrap();
        room_task.abort();

        let _ = self.rooms.remove(&room);

        return format!("[+] Left {room}");
    }

    pub fn get_server_rooms(&self) -> HashSet<String> {
        self.room_manager.get_rooms()
    }

    pub fn update_manager(&mut self, room_manager: Arc<RoomManager>) {
        self.room_manager = room_manager;
    }

    pub async fn recv(&mut self) -> Option<String> {
        self.mpsc_rx.recv().await
    }
}
