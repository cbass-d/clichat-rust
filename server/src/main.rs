use anyhow::{anyhow, Result};
use std::{
    collections::HashMap,
    io,
    sync::{atomic, Arc, Mutex},
    time::Duration,
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
    task::JoinSet,
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

    loop {
        tokio::select! {
            _ = stream_reader.readable() => {
                match stream_reader.try_read_buf(&mut buf) {
                    Ok(len) if len > 0 => {
                        let raw_message = String::from_utf8(buf[0..len].to_vec()).unwrap();
                        println!("{0}", raw_message.clone());
                        if let Some(message) = common::unpack_message(&raw_message) {
                            match message.cmd.as_str() {
                                "register" => {
                                    let name = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::Register {id, name});
                                },
                                "join" => {
                                    let room = message.arg.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::JoinRoom { room, id});
                                },
                                "sendto" => {
                                    let room = message.arg.unwrap();
                                    let content = message.content.unwrap();
                                    let _ = server_request_tx.send(ServerRequest::SendTo {room, content, id});
                                },
                                _ => {},
                            }

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
                    Err(e) => {},
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
                    ServerResponse::Joined{ room } => {
                        let message = common::Message {
                                cmd: String::from("joined"),
                                arg: Some(room),
                                sender: String::from("server"),
                                id: 0,
                                content: None,
                        };
                        let message_response = common::pack_message(message);
                        println!("{message_response}");
                        let _ = stream_writer.write_all(message_response.as_bytes()).await;
                    },
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
                    _ => {},
                }
            },
            message = session_rx.recv() => {
                    println!("{message:?}");
            },
            _ = server_shutdown_rx.recv() => {
                termination = Terminate::ServerClose;
                break;
            }
        }

        buf.clear();
    }

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
    let server_rooms: Vec<Arc<Mutex<Room>>> = vec![Arc::new(Mutex::new(main_room))];
    let room_manager = RoomManager::new(server_rooms);
    println!("[+] Server started...\n[+] Listening for connections...");

    // Initiliaze the needed structures for main loop
    // * Ticker
    // * Client Id (client id will be incremental starting from 1)
    // * HashMap to map client IDs to the clients channel for writing server responses
    // * HashMap to map client IDs to their usernames and vice versa
    // * HashMap to map client IDs to ChatSession structures
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    let mut client_id: u64 = 1;
    let mut client_handles: HashMap<u64, ClientHandle> = HashMap::new();
    let mut username_map: HashMap<String, u64> = HashMap::new();
    let mut ids_map: HashMap<u64, String> = HashMap::new();
    let mut sessions_map: HashMap<u64, ChatSession> = HashMap::new();

    // Main accept/request handler loop
    loop {
        tokio::select! {
            _ = ticker.tick() => {},
            _ = signal::ctrl_c() => {
                break;
            },
            Ok((stream, _)) = listener.accept() => {
                let (response_tx, response_rx) = mpsc::unbounded_channel::<ServerResponse>();
                let (new_session, session_rx) = ChatSession::new(client_id);
                sessions_map.insert(client_id, new_session);

                client_connections.spawn(handle_client(
                    stream,
                    client_id,
                    server_shutdown_rx.resubscribe(),
                    request_tx.clone(),
                    response_rx,
                    session_rx,
                ));
                let client_handle = response_tx;
                client_handles.insert(client_id, client_handle);
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
                                let _ = handle.send(ServerResponse::Failed {error: String::from("Failed to join room")});
                            }
                        }
                    },
                    ServerRequest::SendTo { room, content, id} => {
                        let handle = client_handles.get(&id).unwrap();
                        let session = sessions_map.get_mut(&id).unwrap();

                        match session.rooms.get(&room) {
                            Some((room_handle, _)) => {
                                let _ = room_handle.send_message(content);
                            },
                            None => {
                                let _ = handle.send(ServerResponse::Failed {error: String::from("Not part of room")});
                            }
                        }
                    }
                    _ => {},
                }
            },
        }
    }

    Ok(())
}
