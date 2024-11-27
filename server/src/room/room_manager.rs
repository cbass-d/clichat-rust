use std::{
    collections::HashMap,
    io::Result,
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

    pub async fn drop_user_handle(&self, handle: UserHandle) -> Result<()> {
        let room = self.rooms.get(handle.room());

        match room {
            Some(room) => {
                let mut room = room.lock().unwrap();
                room.leave(handle);
                Ok(())
            }
            None => Err(std::io::Error::into(std::io::ErrorKind::Other.into())),
        }
    }
}
