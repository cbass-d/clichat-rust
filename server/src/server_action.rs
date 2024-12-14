use anyhow::{anyhow, Result};
use tokio::sync::mpsc::{self};

#[derive(Clone)]
pub enum ServerRequest {
    Register {
        id: u64,
        name: String,
    },
    JoinRoom {
        room: String,
        id: u64,
    },
    LeaveRoom {},
    SendTo {
        room: String,
        content: String,
        id: u64,
    },
    DropSession {
        name: String,
        id: u64,
    },
    CreateRoom {
        room: String,
    },
    PrivMsg {
        user_id: u64,
        message: String,
    },
}

pub enum ServerResponse {
    Registered { name: String },
    Joined { room: String },
    Left {},
    Created {},
    Failed { error: String },
}
