use super::{Room, UserHandle};
use anyhow::{anyhow, Result};
use common::message::Message;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast::{self};

pub struct RoomManager {
    rooms: HashMap<String, Arc<Mutex<Room>>>,
}

impl RoomManager {
    pub fn new(rooms: Vec<Arc<Mutex<Room>>>) -> Self {
        RoomManager {
            rooms: rooms
                .into_iter()
                .map(|r| (r.clone().lock().unwrap().name.clone(), r))
                .collect(),
        }
    }

    pub fn add_room(&mut self, room: Arc<Mutex<Room>>, name: String) {
        self.rooms.insert(name, room);
    }

    pub async fn join(&self, room: &str) -> Result<(broadcast::Receiver<Message>, UserHandle)> {
        let room = self.rooms.get(room);

        match room {
            Some(room) => {
                let mut room = room.lock().unwrap();
                let (broadcast_rx, user_handle) = room.join();
                Ok((broadcast_rx, user_handle))
            }
            None => Err(anyhow!("Failed to join room")),
        }
    }

    pub fn get_rooms(&self) -> HashSet<String> {
        HashSet::from_iter(self.rooms.clone().into_keys())
    }
}
