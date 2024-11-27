use std::{
    collections::HashMap,
    io::ErrorKind,
    sync::{Arc, Mutex},
    time::Duration,
};
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

use chat_session::ChatSession;
use room::{room_manager::RoomManager, Room, UserHandle};

mod chat_session;
mod room;
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
    room_manager: Arc<RoomManager>,
) -> Terminate {
    let client_connection = split_stream(stream);
    let (req_handler, mut res_writer) = client_connection;
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chat_session = ChatSession::new("name", room_manager.clone());

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
                                "join" => {
                                    let res = chat_session.join_room(arg.unwrap().to_string()).await;
                                    println!("{res}");
                                },
                                "list" => {
                                    match arg.unwrap() {
                                        "rooms" => {
                                            let rooms: Vec<String> = chat_session.rooms.clone().into_keys().collect();
                                            println!("Rooms:");
                                            for room in rooms {
                                            println!("{room}");
                                            }
                                        },
                                        "users" => {
                                            println!("TODO");
                                        },
                                        _ => {
                                            println!("[-] Invalid option for list");
                                        }
                                    }
                                },
                                "sendto" => {
                                    println!("Sending to {}: {}", arg.unwrap(), message.unwrap());
                                    let user_handle = chat_session.rooms.get(arg.unwrap()).unwrap();
                                    user_handle.send_message(message.unwrap().to_owned());
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
            message = chat_session.recv() => {
                if let Some(message) = message {
                    let _ = res_writer.writer.write_all(message.as_bytes()).await;
                    println!("{message}");
                }
            },
            terminate = shutdown_rx.recv() => {
                break terminate.unwrap();
            }
        }

        buf.clear();
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
    let server_rooms: Vec<Arc<Mutex<Room>>> = vec![Arc::new(Mutex::new(main_room))];
    let room_manager = Arc::new(RoomManager::new(server_rooms));

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
                set.spawn(handle_connection(stream, shutdown_rx.resubscribe(), Arc::clone(&room_manager)));
            }
        }
    }
    set.join_all().await;
}
