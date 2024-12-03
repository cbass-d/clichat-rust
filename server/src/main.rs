use std::{
    collections::HashMap,
    io::ErrorKind,
    sync::{atomic, Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    signal::ctrl_c,
    sync::{
        broadcast::{self},
        mpsc::{self},
    },
    task::JoinSet,
};

use chat_session::ChatSession;
use room::{room_manager::RoomManager, Room};
use server_action::ServerAction;

mod chat_session;
mod room;
mod server_action;
use common;

struct RequestHandler {
    reader: OwnedReadHalf,
}

struct ResponseWriter {
    writer: OwnedWriteHalf,
}

type ClientConnection = (RequestHandler, ResponseWriter);
const SERVER_PORT: &str = "6667";

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
    let addr = format!("0.0.0.0:{SERVER_PORT}");
    let listener = TcpListener::bind(addr).await?;
    return Ok(listener);
}

async fn handle_client(
    stream: TcpStream,
    id: u64,
    mut shutdown_rx: broadcast::Receiver<Terminate>,
    server_action_tx: mpsc::UnboundedSender<ServerAction>,
    mut manager_updates_rx: broadcast::Receiver<Arc<RoomManager>>,
    mut users_updates_rx: broadcast::Receiver<Arc<Mutex<HashMap<String, u64>>>>,
    room_manager: Arc<RoomManager>,
    mut server_users: Arc<Mutex<HashMap<String, u64>>>,
) -> Terminate {
    let client_connection = split_stream(stream);
    let (req_handler, mut res_writer) = client_connection;
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chat_session = ChatSession::new(id, room_manager);

    // Create channel for private messages
    let (private_tx, mut private_rx) = mpsc::unbounded_channel::<String>();
    let _ = server_action_tx.send(ServerAction::AddSession {
        id,
        session_channel: private_tx,
    });

    loop {
        tokio::select! {
            new_manager = manager_updates_rx.recv() => {
                match new_manager {
                    Ok(new_manager) => {
                        chat_session.update_manager(new_manager);
                    }
                    _ => {
                        println!("[-] Failed to update room manager");
                    }
                }
            },
            user_update = users_updates_rx.recv() => {
                match user_update {
                    Ok(user_update) => {
                        server_users = user_update;
                    },
                    _ => {
                        println!("[-] Failed to update users list");
                    }
                }
            }
            _ = req_handler.reader.readable() => {
                match req_handler.reader.try_read_buf(&mut buf) {
                    Ok(len) if len > 0 => {
                        let raw_message = String::from_utf8(buf[0..len].to_vec()).unwrap();
                        if let Some((cmd, arg, sender, id, message)) = common::unpack_message(&raw_message) {
                            match cmd {
                                "register" => {
                                    match arg {
                                        Some(name) => {
                                            let mut response = String::new();
                                            {
                                                let mut server_users = server_users.lock().unwrap();
                                                if server_users.contains_key(name) {
                                                    response = common::pack_message("registered", Some("taken"), "server", 0, Some("0"));
                                                } else {
                                                    server_users.insert(name.to_string(), chat_session.get_id());
                                                    chat_session.set_name(name.to_string());
                                                    response = common::pack_message("registered", Some("success"), "server", 0, Some(&chat_session.get_id().to_string()));
                                                }
                                            }
                                            let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                        },
                                        None => {},
                                    }
                                },
                                "name" => {
                                    let mut response: String;
                                    {
                                        let mut server_users = server_users.lock().unwrap();
                                        let new_name = arg.unwrap();
                                        if new_name != "anon" && server_users.contains_key(new_name) {
                                            response = format!("[-] [{new_name}] as username is already in use");
                                            response = common::pack_message("changedname", Some("failed"), "server", 0, Some(&response));
                                        } else {
                                            let old_name = sender;
                                            let _ = server_users.remove(old_name);
                                            let new_name = new_name.to_string();
                                            server_users.insert(new_name.clone(), chat_session.get_id());
                                            chat_session.set_name(new_name.clone());
                                            response = format!("[+] Username changed to [{new_name}]");
                                            response = common::pack_message("changedname", Some(&new_name), "server", 0, Some(&response));
                                        }
                                    }
                                    let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                },
                                "join" => {
                                    match arg {
                                        Some(arg) => {
                                            let response = chat_session.join_room(arg.to_string()).await;
                                            let response = common::pack_message("joined", Some(arg), "server", 0, Some(&response));
                                            let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                        },
                                        None => {}
                                    }
                                },
                                "leave" => {
                                    match arg {
                                        Some(arg) => {
                                            let response = chat_session.leave_room(arg.to_string());
                                            let response = common::pack_message("left", Some(arg), "server", 0, Some(&response));
                                            let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                        },
                                        None => {},
                                    }
                                }
                                "list" => {
                                    match arg {
                                        Some("rooms") => {
                                            let rooms: Vec<String> = chat_session.rooms.clone().into_keys().collect();
                                            let content = rooms.join(",");
                                            let response = common::pack_message("rooms", None, "server", 0, Some(&content));
                                            let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                        },
                                        Some("users") => {
                                            let mut response = String::new();
                                            {
                                                let server_users = server_users.lock().unwrap();
                                                let mut users_list: Vec<String> = Vec::new();
                                                for (name, id) in server_users.clone().into_iter() {
                                                    let value = format!("{0} {1}", name, id);
                                                    users_list.push(value);
                                                }
                                                response = users_list.join(",");
                                            }
                                            let response = common::pack_message("users", None, "server", 0, Some(&response));
                                            let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                        },
                                        Some("allrooms") => {
                                            let server_rooms = chat_session.get_server_rooms();
                                            let response = Vec::from_iter(server_rooms.into_iter()).into_iter()
                                                .collect::<Vec<String>>().join(",");
                                            let response = common::pack_message("rooms", None, "server", 0, Some(&response));
                                            let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                        },
                                        _ => {}
                                    }
                                },
                                "create" => {
                                    match arg {
                                        Some(new_room) => {
                                            let mut response = String::new();
                                            {
                                                let server_rooms = chat_session.get_server_rooms();
                                                if server_rooms.contains(new_room) {
                                                    response = common::pack_message("created", Some("failed"), "server", 0, None);
                                                }
                                                else {
                                                    let _ = server_action_tx.send(ServerAction::CreateRoom {room: new_room.to_string()});
                                                    response = common::pack_message("created", Some("success"), "server", 0, None);
                                                }
                                            }

                                            let _ = res_writer.writer.write_all(response.as_bytes()).await;
                                        },
                                        None => {},
                                    }
                                },
                                "sendto" => {
                                    match arg {
                                        Some(room) => {
                                            if let Some((user_handle, _)) = chat_session.rooms.get(room) {
                                                match message {
                                                    Some(message) => {
                                                        let server_users = server_users.lock().unwrap();
                                                        let id = *server_users.get(sender).unwrap();
                                                        let message = common::pack_message("roommessage", Some(room), sender, id, Some(message));
                                                        user_handle.send_message(message.to_string());
                                                    },
                                                    None => {},
                                                }
                                            }
                                            else {println!("[-] Room not found: {room}");}
                                        },
                                        None => {},
                                    }
                                },
                                "privmsg" => {
                                    match arg {
                                        Some(user) => {
                                            let server_users = server_users.lock().unwrap();
                                            let user_id = *server_users.get(user).unwrap();
                                            let message = message.unwrap();
                                            let message = common::pack_message("message", None, sender, id.parse().unwrap(), Some(message));
                                            let _ = server_action_tx.send(ServerAction::PrivMsg {user_id, message});
                                        },
                                        None => {},
                                    }
                                },
                                _ => {},
                            }
                        }
                        else { println!("[-] Invalid raw message received"); }
                    },
                    Ok(_) => {
                        println!("[-] Stream closed by client");
                        break;
                    },
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => {},
                    Err(e) => {
                        let err = e.to_string();
                        println!("[-] Failed to read from stream: {err}");
                        break;
                    },
                }
            }
            priv_message = private_rx.recv() => {
                if let Some(message) = priv_message {
                    let _ = res_writer.writer.write_all(message.as_bytes()).await;
                }
            }
            message = chat_session.recv() => {
                if let Some(message) = message {
                    let _ = res_writer.writer.write_all(message.as_bytes()).await;
                }
            },
            _ = shutdown_rx.recv() => {
                break;
            }
        }

        buf.clear();
    }

    let _ = server_action_tx.send(ServerAction::DropSession {
        name: chat_session.get_name(),
        id: chat_session.get_id(),
    });

    Terminate::ClientClosed
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
    let (shutdown_tx, shutdown_rx) = broadcast::channel::<Terminate>(10);

    let main_room = Room::new("main");
    let mut server_rooms: Vec<Arc<Mutex<Room>>> = vec![Arc::new(Mutex::new(main_room))];
    let mut room_manager = Arc::new(RoomManager::new(server_rooms.clone()));
    let mut server_users: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));

    // Channel for performing server wide actions
    let (server_action_tx, mut server_action_rx) = mpsc::unbounded_channel::<ServerAction>();

    // Broadcast channel for handling updates of RoomManager and server users
    let (manager_updates_tx, manager_updates_rx) = broadcast::channel::<Arc<RoomManager>>(10);
    let (users_updates_tx, users_updates_rx) =
        broadcast::channel::<Arc<Mutex<HashMap<String, u64>>>>(10);

    let mut sessions: HashMap<u64, mpsc::UnboundedSender<String>> = HashMap::new();

    // Incremental user id
    // Id '0' will be reserved for respsones orginating from server
    let mut user_id: atomic::AtomicU64 = atomic::AtomicU64::new(1);

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
                let id = user_id.get_mut().clone();
                connections_set.spawn(handle_client(
                    stream,
                    id.into(),
                    shutdown_rx.resubscribe(),
                    server_action_tx.clone(),
                    manager_updates_rx.resubscribe(),
                    users_updates_rx.resubscribe(),
                    Arc::clone(&room_manager),
                    Arc::clone(&server_users))
                );
                user_id = (*user_id.get_mut() + 1).into();
            },
            server_action = server_action_rx.recv() => {
                match server_action{
                    Some(ServerAction::AddSession { id, session_channel }) => {
                        sessions.insert(id, session_channel);
                    },
                    Some(ServerAction::DropSession { name, id }) => {
                        let mut new_server_users = server_users.lock().unwrap().clone();
                        new_server_users.remove(&name);
                        let new_server_users = Arc::new(Mutex::new(new_server_users));
                        server_users = new_server_users;
                        sessions.remove(&id);

                        // Send updated users to client threads
                        let _ = users_updates_tx.send(Arc::clone(&server_users));
                    }
                    Some(ServerAction::CreateRoom { room }) => {
                        let new_room = Room::new(&room);
                        server_rooms.push(Arc::new(Mutex::new(new_room)));
                        let new_manager = Arc::new(RoomManager::new(server_rooms.clone()));
                        room_manager = new_manager;

                        // Send updated room manager to client threads
                        let _ = manager_updates_tx.send(Arc::clone(&room_manager));
                    },
                    Some(ServerAction::PrivMsg { user_id, message }) => {
                        let session = sessions.get(&user_id).unwrap().clone();
                        let _ = session.send(message);
                    },
                    _ => {},
                }

            },
        }
    }
    connections_set.join_all().await;
}
