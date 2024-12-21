use anyhow::{anyhow, Result};
use clap::Parser;
use log::{error, info};
use std::{
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
};

use common;
use server::chat_session::ChatSession;
use server::room::{room_manager::RoomManager, Room};
use server::{Client, ClientEnd, ServerError, ServerRequest, ServerResponse, ServerState};

#[derive(Parser, Debug)]
struct ServerConfig {
    // Port to listening on
    #[arg(short, long)]
    port: u16,
}

fn split_stream(stream: TcpStream) -> (OwnedReadHalf, OwnedWriteHalf) {
    let (reader, writer) = stream.into_split();
    (reader, writer)
}

async fn startup_server(port: u16) -> Result<TcpListener, std::io::Error> {
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(addr).await?;
    return Ok(listener);
}

async fn handle_client(
    id: u64,
    stream: TcpStream,
    mut server_shutdown_rx: broadcast::Receiver<ClientEnd>,
    mut server_response_rx: mpsc::UnboundedReceiver<ServerResponse>,
    mut session_rx: mpsc::UnboundedReceiver<String>,
    server_request_tx: mpsc::UnboundedSender<ServerRequest>,
) -> Result<ClientEnd> {
    let termination: ClientEnd;
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
                                    let username = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::Register { id, username });
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
                                    let username = message.arg.unwrap();
                                    let content = message.content.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::PrivMsg { username, content, id });
                                },
                                "changename" => {
                                    let new_username = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::ChangeName { new_username, id });
                                }
                                _ => {},
                            }

                            // Store command for reporting purposes
                            last_command = message.cmd;
                        }
                        else {
                            info!("[-] Invalid message received");
                        }
                    },
                    Ok(_) => {
                        termination = ClientEnd::ClientClosed;
                        break;
                    },
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {},
                    Err(_) => {
                        termination = ClientEnd::SocketError;
                        break;
                    },
                }
            },
            response = server_response_rx.recv() => {
                match response.unwrap() {
                    ServerResponse::Registered { username } => {
                        let message = common::Message {
                                cmd: String::from("registered"),
                                arg: Some(username.clone()),
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
                    ServerResponse::Messaged { username, content } => {
                        let message = common::Message {
                                cmd: String::from("outgoingmsg"),
                                arg: Some(username),
                                sender: String::from("server"),
                                id,
                                content: Some(content),
                        };

                        let message_response = common::pack_message(message);
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    },
                    ServerResponse::NameChanged { new_username, old_username } => {
                        let message = common::Message {
                                cmd: String::from("changedname"),
                                arg: Some(new_username),
                                sender: String::from("server"),
                                id: 0,
                                content: Some(old_username),
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
                termination = ClientEnd::ServerClose;
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
    // Set RUST_LOG if not already set
    match std::env::var("RUST_LOG") {
        Ok(_) => {}
        Err(_) => {
            std::env::set_var("RUST_LOG", "info");
        }
    };
    env_logger::init();

    // Get server config from cli arguments
    let config = ServerConfig::parse();

    info!("[+] Starting listener...");
    let listener = match startup_server(config.port).await {
        Ok(listener) => listener,
        Err(e) => {
            let e = e.to_string();
            error!("[-] Failed to start server");
            return Err(anyhow!(ServerError::FailedToStart).context(e));
        }
    };

    let mut server_state = ServerState::default();

    // Broadcst channel to handle shut down of server
    let (server_shutdown_tx, server_shutdown_rx) = broadcast::channel::<ClientEnd>(10);

    // Multiple-producer-single-consumer channel for incoming client requests/messages
    // Each client connection will receive a clone of the transmission end
    let (request_tx, mut request_rx) = mpsc::unbounded_channel::<ServerRequest>();

    // Initialize default rooms in server
    let main_room = Room::new("main");
    let default_rooms: Vec<Arc<Mutex<Room>>> = vec![Arc::new(Mutex::new(main_room))];
    let mut room_manager = RoomManager::new(default_rooms);

    info!("[+] Server started");
    info!("[+] Listening at port {0}", config.port);

    // Main accept/request handler loop
    // Sources of events:
    // * Ctrl-c signal to kill server
    // * Request MPSC channel
    // * Client connections join set
    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                let _ = server_shutdown_tx.send(ClientEnd::ServerClose);
                info!("[*] Shutting down server...");
                break;
            },
            res = server_state.connections.join_next() => {
                match res {
                    Some(Ok(_)) => {
                        info!("[-] Client closed");
                    }
                    Some(Err(_)) => {
                        info!("[-] Client crashed");
                    }
                    _ => {},
                }
            },
            Ok((stream, _)) = listener.accept() => {
                let (response_tx, response_rx) = mpsc::unbounded_channel::<ServerResponse>();
                let (new_session, session_rx) = ChatSession::new();
                let id = server_state.get_next_id();
                server_state.increment_id();

                let client = Client::new(id, new_session, response_tx);

                let abort_handle = server_state.connections.spawn(handle_client(
                    id,
                    stream,
                    server_shutdown_rx.resubscribe(),
                    response_rx,
                    session_rx,
                    request_tx.clone(),
                ));

                server_state.add_new_client(client, abort_handle);

                info!("[+] New client connected");
            },
            request = request_rx.recv() => {
                match request.unwrap() {
                    ServerRequest::Register {id, username} => {
                        match server_state.register(id, username.clone()) {
                            Ok(()) => {
                                let client = server_state.clients.get_mut(&id).unwrap();
                                client.username = username.clone();
                            },
                            Err(_) => {},
                        };

                        info!("[+] Client #{} registered as \"{}\"", id, username);
                    },
                    ServerRequest::JoinRoom {room, id} => {
                        if let Some(client) = server_state.clients.get_mut(&id) {
                            client.join_room(room, &room_manager).await;
                        }
                    },
                    ServerRequest::List { opt, id } => {
                        if let Some(client) = server_state.clients.get(&id) {
                            let mut content = String::new();
                            match opt.as_str() {
                                "users" => {
                                    let users = server_state.list_users();
                                    content = users;
                                }
                                "rooms" => {
                                    let session = &client.session;
                                    let user_rooms: String = session.rooms.clone().into_keys().map(|s| s.to_string()).collect::<Vec<String>>().join(",");
                                    content = user_rooms;
                                }
                                "allrooms" => {
                                    let all_rooms = room_manager.get_rooms();
                                    let all_rooms: String = all_rooms.into_iter().map(|s| s.to_string()).collect::<Vec<String>>().join(",");
                                    content = all_rooms;
                                }
                                _ => {},
                            }

                            let _ = client.handle.send(ServerResponse::Listing { opt, content });
                        }
                    },
                    ServerRequest::CreateRoom { room, id } => {
                        if let Some(client) = server_state.clients.get(&id) {
                            if room_manager.rooms.contains_key(&room) {
                                let _ = client.handle.send(ServerResponse::Failed { error: ServerError::RoomAlreadyExists.to_string()});
                            }
                            else {
                                let new_room = Arc::new(Mutex::new(Room::new(&room)));
                                room_manager.add_room(new_room, room.clone());

                                let _ = client.handle.send(ServerResponse::CreatedRoom { room: room.clone() });

                                info!("[+] New {} room created by client #{}", room, id);
                            }
                        }
                    },
                    ServerRequest::LeaveRoom { room, id } => {
                        if let Some(client) = server_state.clients.get_mut(&id) {
                            client.leave_room(room, &room_manager);
                        }
                    },
                    ServerRequest::SendTo { room, content, id} => {
                        if let Some(client) = server_state.clients.get(&id) {
                            let session = &client.session;
                            match session.rooms.get(&room) {
                                Some((room_handle, _)) => {
                                     let message = common::Message {
                                        cmd: String::from("roommessage"),
                                        arg: Some(room),
                                        sender: client.username.clone(),
                                        id,
                                        content: Some(content),
                                    };
                                    let message = common::pack_message(message);
                                    let _ = room_handle.send_message(message);
                                },
                                None => {
                                    let _ = client.handle.send(ServerResponse::Failed { error: ServerError::NotPartOfRoom.to_string()});
                                }

                            }
                        }
                    },
                    ServerRequest::PrivMsg { username, content, id } => {
                        if let Some(client) = server_state.clients.get(&id) {
                            let sender = client.username.clone();
                            if let Some(receiver_id) = server_state.get_user_id(&username) {
                                let receiver = server_state.clients.get(&receiver_id).unwrap();
                                let receiver_session = &receiver.session;
                                let message = common::Message {
                                    cmd: String::from("incomingmsg"),
                                    arg: None,
                                    sender: sender.to_string(),
                                    id,
                                    content: Some(content.clone()),
                                };

                                let message = common::pack_message(message);
                                let _ = receiver_session.mpsc_tx.send(message);

                                let _ = client.handle.send(ServerResponse::Messaged {username, content });
                            }
                            else {
                                let _ = client.handle.send(ServerResponse::Failed { error: ServerError::UserNotFound.to_string()});
                            }

                        }
                    },
                    ServerRequest::ChangeName { new_username, id } => {
                        match server_state.change_username(id, new_username.clone()) {
                            Ok(()) => {
                                let client = server_state.clients.get_mut(&id).unwrap();
                                let old_username = client.username.clone();
                                client.username = new_username.clone();

                                info!("[+] Client #{} changed name from \"{}\" to \"{}\"", id, old_username, new_username);
                            }
                            Err(_) => {},
                        }

                    },
                    ServerRequest::DropSession { id } => {
                        if let Some(_) = server_state.clients.get(&id) {
                            server_state.drop_client(id);

                            info!("[+] Dropped session");
                        }
                    },
                }
            },
        }
    }

    info!("[+] Server shut down");

    Ok(())
}
