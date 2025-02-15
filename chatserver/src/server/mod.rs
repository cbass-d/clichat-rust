mod server_events;
mod session;

use common::message::{Message, MessageType};
use server_events::{handle_event, ServerEvent, ServerReply};
pub use session::Session;

use crate::room::{room_manager::RoomManager, Room};
use anyhow::Result;
use log::{error, info};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tokio::{
    net::TcpListener,
    sync::{
        broadcast::{self},
        mpsc::{self},
        oneshot::{self},
    },
    task::JoinSet,
};

type SessionHandle = mpsc::UnboundedSender<Message>;

pub struct Server {
    next_client_id: AtomicU64,
    port: u64,
    username_to_id: HashMap<String, u64>,
    id_to_username: HashMap<u64, String>,
    room_manager: RoomManager,
    to_server_tx: mpsc::UnboundedSender<(ServerEvent, oneshot::Sender<ServerReply>)>,
    rx: mpsc::UnboundedReceiver<(ServerEvent, oneshot::Sender<ServerReply>)>,
    sessions: HashMap<u64, (Session, SessionHandle)>,
    session_tasks: JoinSet<()>,
}

impl Server {
    pub fn new(port: u64) -> Self {
        let default_rooms: Vec<Arc<Mutex<Room>>> = vec![Arc::new(Mutex::new(Room::new("main")))];
        let room_manager = RoomManager::new(default_rooms);
        let (to_server_tx, rx) =
            mpsc::unbounded_channel::<(ServerEvent, oneshot::Sender<ServerReply>)>();

        Self {
            next_client_id: AtomicU64::new(1),
            port,
            username_to_id: HashMap::new(),
            id_to_username: HashMap::new(),
            room_manager,
            to_server_tx,
            rx,
            sessions: HashMap::new(),
            session_tasks: JoinSet::new(),
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(addr).await?;
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        info!("[+] Server started");
        info!("[+] Listening at port {0}", self.port);

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    let _ = shutdown_tx.send(());
                    break;
                },
                result = listener.accept() => match result {
                    Ok((stream, _)) => {
                        info!("[*] New connection");

                        // Spawn new thread in join set
                        self.session_tasks.spawn({
                            let to_server_tx = self.to_server_tx.clone();

                            let id = self.next_client_id.fetch_add(1, Ordering::Relaxed);
                            let (to_session_tx, session_rx, session) = Session::new();
                            let session_shutdown_rx = shutdown_rx.resubscribe();

                            self.sessions.insert(id, (session, to_session_tx));

                            async move {
                                let _ = session::handle_session(
                                        id,
                                        stream,
                                        to_server_tx,
                                        session_rx,
                                        session_shutdown_rx
                                    ).await;
                            }
                        });
                    },
                    Err(e) => {
                        error!("[-] Failed to accept new connection: {}", e);
                    }
                },
                server_request = self.rx.recv() => {
                    if let Some((event, reply_tx)) = server_request {
                        let _ = handle_event(event, reply_tx, self).await;
                    }
                },
            }
        }

        Ok(())
    }

    pub async fn close_server(self) {
        info!("[*] Closing server");

        self.session_tasks.join_all().await;
    }
}
