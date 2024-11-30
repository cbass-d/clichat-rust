use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::{
    sync::mpsc::{self},
    task::JoinSet,
};

use super::room::room_manager::RoomManager;
use super::room::UserHandle;

pub struct ChatSession {
    name: String,
    pub rooms: HashMap<String, UserHandle>,
    room_manager: Arc<RoomManager>,
    room_task_set: JoinSet<()>,
    mpsc_tx: mpsc::Sender<String>,
    mpsc_rx: mpsc::Receiver<String>,
}

impl ChatSession {
    pub fn new(name: &str, room_manager: Arc<RoomManager>) -> Self {
        let (mpsc_tx, mpsc_rx) = mpsc::channel(10);

        Self {
            rooms: HashMap::new(),
            room_manager,
            name: name.to_owned(),
            room_task_set: JoinSet::new(),
            mpsc_tx,
            mpsc_rx,
        }
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

            self.rooms.insert(room.clone(), user_handle);

            return format!("[+] Joined {room}");
        }

        format!("[-] Failed to join {room}")
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
