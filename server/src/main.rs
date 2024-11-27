use std::{collections::HashMap, io::ErrorKind, sync::Arc, time::Duration};
use tokio::{
    io::AsyncWriteExt,
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    signal::ctrl_c,
    sync::broadcast::{error::TryRecvError, Receiver},
    task::JoinSet,
};

use chat_session::{ChatSession, Command, Room};

mod chat_session;
use common;

struct RequestHandler {
    reader: OwnedReadHalf,
}

struct ResponseWriter {
    writer: OwnedWriteHalf,
}

type ClientConnection = (RequestHandler, ResponseWriter);
const SERVER_ADDR: &str = "127.0.0.1";

#[derive(Clone)]
enum Terminate {
    ClientClosed,
    SocketError,
    ServerClose,
}

fn split_stream(stream: TcpStream) -> ClientConnection {
    let (reader, writer) = stream.into_split();
    (RequestHandler { reader }, ResponseWriter { writer })
}

async fn startup_server() -> Result<TcpListener, std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:6667").await?;
    return Ok(listener);
}

async fn handle_connection(
    stream: TcpStream,
    mut shutdown_rx: Receiver<Terminate>,
    mut rooms_map: Arc<HashMap<String, Room>>,
) -> Terminate {
    let client_connection = split_stream(stream);
    let (req_handler, mut res_writer) = client_connection;
    let mut buf: Vec<u8> = Vec::with_capacity(4096);

    let mut chat_session = ChatSession::new();

    loop {
        match shutdown_rx.try_recv() {
            Ok(msg) => {
                break msg;
            }
            Err(TryRecvError::Closed) => {
                break Terminate::ServerClose;
            }
            Err(TryRecvError::Empty) => {}
            Err(_) => {}
        }

        tokio::select! {
            _ = req_handler.reader.readable() => {
                match req_handler.reader.try_read_buf(&mut buf) {
                    Ok(len) if len > 0 => {
                        let msg = String::from_utf8(buf[0..len].to_vec()).unwrap();
                        println!("{msg}");
                        if let Some((origin, cmd, arg, sender, message)) = common::unpack_message(&msg) {
                            match cmd {
                                "join" => {},
                                "list" => {},
                                "sendto" => {
                                    println!("Sending to {}, {}", arg.unwrap(), message.unwrap());
                                },
                                _ => {
                                    println!("[-] Invalid command received");
                                },
                            }
                        }
                        else { println!("[-] Invalid message received"); }
                    },
                    Ok(_) => {
                        println!("[-] Stream closed by client");
                        break Terminate::ClientClosed;
                    },
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => {},
                    Err(e) => {
                        let err = e.to_string();
                        println!("[-] Failed to read from stream: {err}");
                        break Terminate::SocketError;
                    },
                }
            }
            msg = chat_session.recv() => {
                if let Some(msg) = msg {
                    println!("[+] msg");
                }
            },
        }
    }
}

#[tokio::main]
async fn main() {
    println!("[+] Starting listener...");
    let listener = match startup_server().await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("[-] Failed to start server: {e}");
            panic!();
        }
    };
    let mut set: JoinSet<Terminate> = JoinSet::new();
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<Terminate>(1);
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    let main_room = Room::new("main");
    let mut rooms_map = HashMap::new();
    rooms_map.insert("main".to_string(), main_room);
    let rooms_map: Arc<HashMap<String, Room>> = Arc::new(rooms_map);
    let root_rooms_map = Arc::clone(&rooms_map);

    println!("[+] Server started...\n[+] Listening for connections...");

    loop {
        tokio::select! {
            _ = ticker.tick() => {},
            Ok(_) = ctrl_c() => {
                println!("[-] Shutting down server");
                let _ = shutdown_tx.send(Terminate::ServerClose);
                break;
            },
            Ok((stream, _addr)) = listener.accept() => {
                set.spawn(handle_connection(stream, shutdown_rx.resubscribe(), Arc::clone(&rooms_map)));
            }
        }
    }

    set.join_all().await;
}
