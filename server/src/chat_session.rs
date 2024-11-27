use tokio::{
    io::Result,
    sync::broadcast::{self},
    sync::mpsc::{self},
};

pub enum Command {
    List { opt: String },
    Join { room: String },
}

pub struct Room {
    pub name: String,
    pub broadcast_tx: broadcast::Sender<String>,
}

impl Room {
    pub fn new(name: &str) -> Self {
        let (broadcast_tx, broadcast_rx) = broadcast::channel(50);

        Room {
            name: name.to_owned(),
            broadcast_tx,
        }
    }

    pub fn join(&mut self) -> (broadcast::Receiver<String>, broadcast::Sender<String>) {
        let broadcast_tx = self.broadcast_tx.clone();
        let broadcast_rx = self.broadcast_tx.subscribe();

        (broadcast_rx, broadcast_tx)
    }
}

pub struct ChatSession {
    pub rooms: Vec<Room>,
    mpsc_tx: mpsc::Sender<String>,
    mpsc_rx: mpsc::Receiver<String>,
}

impl ChatSession {
    pub fn new() -> Self {
        let (mpsc_tx, mpsc_rx) = mpsc::channel(10);

        Self {
            rooms: Vec::new(),
            mpsc_tx,
            mpsc_rx,
        }
    }

    pub async fn recv(&mut self) -> Option<String> {
        self.mpsc_rx.recv().await
    }
}
