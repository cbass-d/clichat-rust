use anyhow::{anyhow, Result};
use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
};
use tokio::{
    io::AsyncWriteExt,
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    signal::{self},
    sync::{
        broadcast::{self},
        mpsc::{self},
    },
    task::{AbortHandle, JoinSet},
};

use chat_session::ChatSession;
use room::{room_manager::RoomManager, Room};
use server_action::{ServerRequest, ServerResponse};

mod chat_session;
mod room;
mod server_action;
use common;

const SERVER_PORT: &str = "6667";
type ClientHandle = mpsc::UnboundedSender<ServerResponse>;

#[derive(Clone)]
enum Terminate {
    ClientClosed,
    SocketError,
    ServerClose,
}

fn split_stream(stream: TcpStream) -> (OwnedReadHalf, OwnedWriteHalf) {
    let (reader, writer) = stream.into_split();
    (reader, writer)
}

async fn startup_server() -> Result<TcpListener, std::io::Error> {
    let addr = format!("0.0.0.0:{SERVER_PORT}");
    let listener = TcpListener::bind(addr).await?;
    return Ok(listener);
}

async fn handle_client(
    stream: TcpStream,
    id: u64,
    mut server_shutdown_rx: broadcast::Receiver<Terminate>,
    server_request_tx: mpsc::UnboundedSender<ServerRequest>,
    mut server_response_rx: mpsc::UnboundedReceiver<ServerResponse>,
    mut session_rx: mpsc::UnboundedReceiver<String>,
) -> Result<Terminate> {
    let termination: Terminate;
    let (stream_reader, mut stream_writer) = split_stream(stream);
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut last_command: String = String::new();

    // Sources of events
    // * Client Tcp stream
    // * Server response MPSC channel
    // * Client session MPSC channel
    // * Server shutdown broadcast channel
    loop {
        tokio::select! {
            _ = stream_reader.readable() => {
                match stream_reader.try_read_buf(&mut buf) {
                    Ok(len) if len > 0 => {
                        let raw_message = String::from_utf8(buf[0..len].to_vec()).unwrap();
                        if let Some(message) = common::unpack_message(&raw_message) {
                            match message.cmd.as_str() {
                                "register" => {
                                    let name = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::Register { id, name });
                                },
                                "join" => {
                                    let room = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::JoinRoom { room, id });
                                },
                                "sendto" => {
                                    let room = message.arg.unwrap();
                                    let content = message.content.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::SendTo { room, content, id });
                                },
                                "list" => {
                                    let opt = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::List { opt, id });
                                },
                                "create" => {
                                    let room = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::CreateRoom { room, id });
                                },
                                "leave" => {
                                    let room = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::LeaveRoom { room, id });
                                },
                                "privmsg" => {
                                    let user = message.arg.unwrap();
                                    let content = message.content.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::PrivMsg { user, content, id });
                                },
                                "changename" => {
                                    let new_name = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::ChangeName { new_name, id });
                                }
                                _ => {},
                            }

                            // Store command for reporting purposes
                            last_command = message.cmd;
                        }
                        else {
                            println!("[-] Invalid message received");
                        }
                    },
                    Ok(_) => {
                        termination = Terminate::ClientClosed;
                        break;
                    },
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {},
                    Err(_) => {
                        termination = Terminate::SocketError;
                        break;
                    },
                }
            },
            response = server_response_rx.recv() => {
                match response.unwrap() {
                    ServerResponse::Registered { name } => {
                        let message = common::Message {
                                cmd: String::from("registered"),
                                arg: Some(name.clone()),
                                sender: String::from("server"),
                                id: 0,
                                content: Some(id.to_string()),
                        };

                        let message_response = common::pack_message(message);
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    },
                    ServerResponse::Joined { room } => {
                        let message = common::Message {
                                cmd: String::from("joined"),
                                arg: Some(room),
                                sender: String::from("server"),
                                id: 0,
                                content: None,
                        };

                        let message_response = common::pack_message(message);
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    },
                    ServerResponse::Listing { opt, content } => {
                        let message = common::Message {
                                cmd: opt,
                                arg: None,
                                sender: String::from("server"),
                                id: 0,
                                content: Some(content),
                        };

                        let message_response = common::pack_message(message);
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    },
                    ServerResponse::CreatedRoom { room } => {
                        let message = common::Message {
                                cmd: String::from("createdroom"),
                                arg: Some(room),
                                sender: String::from("server"),
                                id: 0,
                                content: None,
                        };

                        let message_response = common::pack_message(message);
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    },
                    ServerResponse::LeftRoom { room } => {
                        let message = common::Message {
                            cmd: String::from("leftroom"),
                            arg: Some(room),
                            sender: String::from("server"),
                            id: 0,
                            content: None,
                        };

                        let message_response = common::pack_message(message);
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    },
                    ServerResponse::Messaged { user, content } => {
                        let message = common::Message {
                                cmd: String::from("outgoingmsg"),
                                arg: Some(user),
                                sender: String::from("server"),
                                id,
                                content: Some(content),
                        };

                        let message_response = common::pack_message(message);
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    },
                    ServerResponse::NameChanged { new_name, old_name } => {
                        let message = common::Message {
                                cmd: String::from("changedname"),
                                arg: Some(new_name),
                                sender: String::from("server"),
                                id: 0,
                                content: Some(old_name),
                        };

                        let message_response = common::pack_message(message);
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    }
                    ServerResponse::Failed { error } => {
                        let message = common::Message {
                                cmd: String::from("failed"),
                                arg: Some(last_command.clone()),
                                sender: String::from("server"),
                                id: 0,
                                content: Some(error),
                        };

                        let message_response = common::pack_message(message);
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    },
                }
            },
            message = session_rx.recv() => {
                if let Some(message) = message {
                        let _ = stream_writer.write_all(message.as_bytes()).await;
                }
            },
            _ = server_shutdown_rx.recv() => {
                termination = Terminate::ServerClose;
                break;
            }
        }

        buf.clear();
    }

    let _ = server_request_tx.send(ServerRequest::DropSession { id });

    Ok(termination)
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("[+] Starting listener...");
    let listener = match startup_server().await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("[-] Failed to start server: {e}");
            return Err(anyhow!("Failed to start server"));
        }
    };

    // * Set of tasks/threads for each incoming client connection
    // * Broadcst channel to handle shut down of server
    // * Multiple-producer-single-consumer channel for incoming client requests/messages
    let mut client_connections: JoinSet<Result<Terminate>> = JoinSet::new();
    let (server_shutdown_tx, server_shutdown_rx) = broadcast::channel::<Terminate>(10);
    let (request_tx, mut request_rx) = mpsc::unbounded_channel::<ServerRequest>();

    // Initialize default rooms in server
    let main_room = Room::new("main");
    let default_rooms: Vec<Arc<Mutex<Room>>> = vec![Arc::new(Mutex::new(main_room))];
    let mut room_manager = RoomManager::new(default_rooms);
    println!("[+] Server started...\n[+] Listening for connections...");

    // Initiliaze the needed structures for main loop
    // * Ticker
    // * Client Id (client id will be incremental starting from 1)
    // * HashMap to map client IDs to the clients channel for writing server responses
    // * HashMap to map client IDs to their usernames and vice versa
    // * HashMap to map client IDs to ChatSession structures
    // * HashMap for client task abort handles
    let mut client_id: u64 = 1;
    let mut client_handles: HashMap<u64, ClientHandle> = HashMap::new();
    let mut username_map: HashMap<String, u64> = HashMap::new();
    let mut ids_map: HashMap<u64, String> = HashMap::new();
    let mut sessions_map: HashMap<u64, ChatSession> = HashMap::new();
    let mut abort_handles: HashMap<u64, AbortHandle> = HashMap::new();

    // Main accept/request handler loop
    // Sources of events:
    // * Ctrl-c signal to kill server
    // * Request MPSC channel
    // * Client connections join set
    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                let _ = server_shutdown_tx.send(Terminate::ServerClose);
                break;
            },
            res = client_connections.join_next() => {
                match res {
                    Some(Ok(_)) => {
                        println!("[-] Client closed");
                    }
                    Some(Err(_)) => {
                        println!("[-] Client crashed");
                    }
                    _ => {},
                }
            },
            Ok((stream, _)) = listener.accept() => {
                let (response_tx, response_rx) = mpsc::unbounded_channel::<ServerResponse>();
                let (new_session, session_rx) = ChatSession::new();
                sessions_map.insert(client_id, new_session);

                let abort_handle = client_connections.spawn(handle_client(
                    stream,
                    client_id,
                    server_shutdown_rx.resubscribe(),
                    request_tx.clone(),
                    response_rx,
                    session_rx,
                ));
                let client_handle = response_tx;
                client_handles.insert(client_id, client_handle);
                abort_handles.insert(client_id, abort_handle);
                client_id += 1;
            },
            request = request_rx.recv() => {
                match request.unwrap() {
                    ServerRequest::Register {id, name} => {
                        let handle = client_handles.get(&id).unwrap();
                        if username_map.contains_key(&name) {
                            let _ = handle.send(ServerResponse::Failed {error: String::from("Name is already taken")});
                        }
                        else {
                            username_map.insert(name.clone(), id);
                            ids_map.insert(id, name.clone());
                            let session = sessions_map.get_mut(&id).unwrap();
                            session.set_name(name.clone());
                            let _ = handle.send(ServerResponse::Registered { name: name.clone() });
                        }
                    },
                    ServerRequest::JoinRoom {room, id} => {
                        let handle = client_handles.get(&id).unwrap();
                        let user = ids_map.get(&id).unwrap();
                        let session = sessions_map.get_mut(&id).unwrap();

                        if session.rooms.contains_key(&room) {
                            let _ = handle.send(ServerResponse::Failed {error: String::from("Already part of room")});
                            continue;
                        }

                        match room_manager.join(&room, user).await {
                            Some((mut broadcast_rx, user_handle)) => {

                                let room_task = session.room_task_set.spawn({
                                    let mpsc_tx = session.mpsc_tx.clone();

                                    async move {
                                        while let Ok(message) = broadcast_rx.recv().await {
                                            let _ = mpsc_tx.send(message);
                                        }
                                    }
                                });

                                session.rooms.insert(room.clone(), (user_handle, room_task));

                                let _ = handle.send(ServerResponse::Joined { room });
                            },
                            None => {
                                let _ = handle.send(ServerResponse::Failed { error: String::from("Failed to join room") });
                            }
                        }
                    },
                    ServerRequest::List { opt, id } => {
                        let handle = client_handles.get(&id).unwrap();
                        let session = sessions_map.get_mut(&id).unwrap();

                        match opt.as_str() {
                            "rooms" => {
                                let user_rooms: Vec<String> = session.rooms.keys().into_iter().map(|k| k.to_string()).collect();
                                let user_rooms = user_rooms.join(",");

                                let _ = handle.send(ServerResponse::Listing { opt, content: user_rooms });
                            },
                            "allrooms" => {
                                let all_rooms = room_manager.get_rooms();
                                let all_rooms: String = all_rooms.into_iter()
                                    .map(|s| s.to_string())
                                    .collect::<Vec<String>>()
                                    .join(",");

                                let _ = handle.send(ServerResponse::Listing { opt, content: all_rooms });
                            },
                            "users" => {
                                let server_users: Vec<String> = username_map.keys().into_iter().map(|k| k.to_string()).collect();
                                let server_users = server_users.join(",");

                                let _ = handle.send(ServerResponse::Listing { opt, content: server_users });
                            },
                            _ => {
                                let _ = handle.send(ServerResponse::Failed { error: String::from("Not a valid argument") });
                            },
                        }
                    },
                    ServerRequest::CreateRoom { room, id } => {
                        let handle = client_handles.get(&id).unwrap();
                        let server_rooms = room_manager.get_rooms();

                        if server_rooms.contains(&room) {
                            let _ = handle.send(ServerResponse::Failed { error: String::from("Room already exists") });
                        }
                        else {
                            let new_room = Arc::new(Mutex::new(Room::new(&room)));
                            room_manager.add_room(new_room, room.clone());

                            let _ = handle.send(ServerResponse::CreatedRoom { room });
                        }
                    },
                    ServerRequest::LeaveRoom { room, id } => {
                        let handle = client_handles.get(&id).unwrap();
                        let session = sessions_map.get_mut(&id).unwrap();

                        if session.rooms.contains_key(&room) {
                            let room_name = room.clone();

                            // Remove user from Room inside RoomManager by consuming the
                            let room = room_manager.rooms.get_mut(&room).unwrap();
                            let mut room = room.lock().unwrap();
                            room.leave(session.get_name());

                            // Remove room from client session
                            let _ = session.leave_room(room_name.clone());

                            let _ = handle.send(ServerResponse::LeftRoom { room: room_name });
                        }
                        else {
                            let _ = handle.send(ServerResponse::Failed { error: String::from("Not part of room") });
                        }
                    },
                    ServerRequest::SendTo { room, content, id} => {
                        let handle = client_handles.get(&id).unwrap();
                        let session = sessions_map.get_mut(&id).unwrap();

                        match session.rooms.get(&room) {
                            Some((room_handle, _)) => {
                                let message = common::Message {
                                    cmd: String::from("roommessage"),
                                    arg: Some(room),
                                    sender: session.get_name(),
                                    id,
                                    content: Some(content),
                                };
                                let message = common::pack_message(message);
                                let _ = room_handle.send_message(message);
                            },
                            None => {
                                let _ = handle.send(ServerResponse::Failed { error: String::from("Not part of room") });
                            }
                        }
                    },
                    ServerRequest::PrivMsg { user, content, id } => {
                        let handle = client_handles.get(&id).unwrap();
                        let sender = ids_map.get(&id).unwrap();
                        if let Some(receiver_id) = username_map.get(&user) {
                            let receiver_session = sessions_map.get(&receiver_id).unwrap();
                            let message = common::Message {
                                cmd: String::from("incomingmsg"),
                                arg: None,
                                sender: sender.to_string(),
                                id,
                                content: Some(content.clone()),
                            };

                            let message = common::pack_message(message);
                            let _ = receiver_session.mpsc_tx.send(message);

                            let _ = handle.send(ServerResponse::Messaged { user, content });
                        }
                        else {
                            let _ = handle.send(ServerResponse::Failed { error: String::from("User not found") });
                        }
                    },
                    ServerRequest::ChangeName { new_name, id } => {
                        let handle = client_handles.get(&id).unwrap();
                        if username_map.contains_key(&new_name) {
                            let _ = handle.send(ServerResponse::Failed { error: String::from("Username is already taken") });
                        }
                        else {
                            let session = sessions_map.get_mut(&id).unwrap();
                            let old_name = session.get_name();

                            // Update data structures
                            username_map.remove(&old_name);
                            username_map.insert(new_name.clone(), id);
                            *ids_map.get_mut(&id).unwrap() = new_name.clone();
                            session.set_name(new_name.clone());

                            let _ = handle.send(ServerResponse::NameChanged { new_name, old_name });
                        }

                    },
                    ServerRequest::DropSession { id } => {
                        // When register call fails info is not stored
                        if ids_map.contains_key(&id) {
                            let user = ids_map.get(&id).unwrap();
                            let user = user.clone();
                            ids_map.remove(&id);
                            username_map.remove(&user);
                            sessions_map.remove(&id);
                            client_handles.remove(&id);
                        }

                        let abort_handle = abort_handles.get(&id).unwrap();
                        abort_handle.abort();
                    },
                }
            },
        }
    }

    Ok(())
}
