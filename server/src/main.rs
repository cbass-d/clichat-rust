use std::{
    collections::HashSet,
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
use room::{room_manager::RoomManager, Room};

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

async fn handle_client(
    stream: TcpStream,
    mut shutdown_rx: Receiver<Terminate>,
    room_manager: Arc<RoomManager>,
    server_users: Arc<Mutex<HashSet<String>>>,
) -> Terminate {
    let client_connection = split_stream(stream);
    let (req_handler, mut res_writer) = client_connection;
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chat_session = ChatSession::new("name", room_manager.clone());

    loop {
        match shutdown_rx.try_recv() {
            Ok(termination) => {
                break termination;
            }
            Err(TryRecvError::Closed) => {
                break Terminate::ServerClose;
            }
            Err(_) => {}
        }

        tokio::select! {
            _ = req_handler.reader.readable() => {
                match req_handler.reader.try_read_buf(&mut buf) {
                    Ok(len) if len > 0 => {
                        let raw_message = String::from_utf8(buf[0..len].to_vec()).unwrap();
                        if let Some((cmd, arg, sender, message)) = common::unpack_message(&raw_message) {
                            match cmd {
                                "register" => {
                                    match arg {
                                        Some(name) => {
                                            let mut response = String::new();
                                            {
                                                let mut server_users = server_users.lock().unwrap();
                                                if server_users.contains(name) {
                                                    response = format!("[-] [{name}] as username is already in use");
                                                    response = common::pack_message("registered", Some("failed"), "server", Some(&response));
                                                } else {
                                                    server_users.insert(name.to_string());
                                                    response = format!("[+] Registered as {name}");
                                                    response = common::pack_message("registered", Some("success"), "server", Some(&response));
                                                }
                                            }
                                            let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                        },
                                        None => {
                                            let message = "[-] Invalid or no argument received";
                                            let message = common::pack_message("registered", Some("failed"), "server", Some(&message));
                                            let _ = res_writer.writer.write_all(message.as_bytes()).await;
                                        },
                                    }
                                }
                                "name" => {
                                    let mut response: String;
                                    if arg == None {
                                        response = "[-] Invalid request recieved".to_string();
                                        response = common::pack_message("name", None, "server", Some(&response));
                                    } else {
                                        let mut server_users = server_users.lock().unwrap();
                                        let new_name = arg.unwrap();
                                        if server_users.contains(new_name) {
                                            response = format!("[-] [{new_name}] as username is already in use");
                                            response = common::pack_message("changedname", Some("failed"), "server", Some(&response));
                                        } else {
                                            let old_name = sender;
                                            let _ = server_users.remove(old_name);
                                            server_users.insert(new_name.to_string());
                                            response = format!("[+] Username changed to [{new_name}]");
                                            response = common::pack_message("changedname", Some(new_name), "server", Some(&response));
                                        }
                                    }
                                    let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                }
                                "join" => {
                                    match arg {
                                        Some(arg) => {
                                            let result = chat_session.join_room(arg.to_string()).await;
                                            let message = common::pack_message("joined", Some(arg), "server", Some(&result));
                                            let _ = res_writer.writer.write_all(message.as_bytes()).await;
                                        },
                                        None => {
                                            println!("[-] Invalid argument");
                                        }
                                    }
                                },
                                "list" => {
                                    match arg {
                                        Some("rooms") => {
                                            let rooms: Vec<String> = chat_session.rooms.clone().into_keys().collect();
                                            let content = rooms.join(",");
                                            let message = common::pack_message("rooms", None, "server", Some(&content));
                                            let _ = res_writer.writer.write_all(message.as_bytes()).await;
                                        },
                                        Some("users") => {
                                            let mut response = String::new();
                                            {
                                                let server_users = server_users.lock().unwrap();
                                                response = Vec::from_iter(server_users.clone().into_iter()).into_iter().collect::<Vec<String>>().join(",");
                                            }
                                            let response = common::pack_message("users", None, "server", Some(&response));
                                            let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                        },
                                        _ => {
                                            println!("[-] Invalid option for list");
                                        }
                                    }
                                },
                                "sendto" => {
                                    match arg {
                                        Some(room) => {
                                            if let Some(user_handle) = chat_session.rooms.get(room) {
                                                match message {
                                                    Some(message) => {
                                                        let message = common::pack_message("roommessage", Some(room), sender, Some(message));
                                                        user_handle.send_message(message.to_string());
                                                    },
                                                    None => {
                                                        println!("[-] No message provided");
                                                    }
                                                }
                                            }
                                            else {println!("[-] Room not found: {room}");}
                                        },
                                        None => {
                                            println!("[-] Invalid argument provided");
                                        },
                                    }
                                },
                                _ => {
                                    println!("[-] Invalid command received");
                                },
                            }
                        }
                        else { println!("[-] Invalid raw message received"); }
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
    let mut connections_set: JoinSet<Terminate> = JoinSet::new();
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<Terminate>(10);

    // Default server rooms
    let main_room = Room::new("main");
    let server_rooms: Vec<Arc<Mutex<Room>>> = vec![Arc::new(Mutex::new(main_room))];
    let room_manager = Arc::new(RoomManager::new(server_rooms));

    // Users
    let server_users: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    println!("[+] Server started...\n[+] Listening for connections...");

    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    loop {
        tokio::select! {
            _ = ticker.tick() => {},
            Ok(_) = ctrl_c() => {
                println!("[-] Shutting down server");
                let _ = shutdown_tx.send(Terminate::ServerClose);
                break;
            },
            Ok((stream, _)) = listener.accept() => {
                connections_set.spawn(handle_client(stream, shutdown_rx.resubscribe(), Arc::clone(&room_manager), Arc::clone(&server_users)));
            }
        }
    }
    connections_set.join_all().await;
}
