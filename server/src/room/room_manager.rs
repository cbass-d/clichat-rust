use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast::{self};

use super::{Room, UserHandle};

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

    pub async fn join(
        &self,
        name: &str,
        user: &str,
    ) -> Option<(broadcast::Receiver<String>, UserHandle)> {
        let room = self.rooms.get(name);

        match room {
            Some(room) => {
                let mut room = room.lock().unwrap();
                let (broadcast_rx, user_handle) = room.join(user.to_owned());
                Some((broadcast_rx, user_handle))
            }
            None => {
                return None;
            }
        }
    }

    pub fn get_rooms(&self) -> HashSet<String> {
        HashSet::from_iter(self.rooms.clone().into_keys().into_iter())
    }
}
