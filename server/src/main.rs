use std::io::ErrorKind;
use std::time::Duration;
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

async fn handle_connection(stream: TcpStream, mut shutdown_rx: Receiver<Terminate>) -> Terminate {
    let client_connection = split_stream(stream);
    let (req_handler, mut res_writer) = client_connection;
    let mut buf = Vec::with_capacity(4096);

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

        match req_handler.reader.try_read_buf(&mut buf) {
            Ok(len) if len > 0 => {
                let message = String::from_utf8(buf[0..len].to_vec()).unwrap();
                if let Some((origin, room_option, sender, message)) =
                    common::unpack_message(&message)
                {
                    if let Some(room) = room_option {
                        println!("From {origin}-{sender} to {room:?}: {message}");
                    } else {
                        println!("From {origin}-{sender}: {message}");
                    }
                    let packed_message =
                        common::pack_message(SERVER_ADDR, Some("echo"), "server", message);
                    let _ = res_writer.writer.write_all(packed_message.as_bytes()).await;
                    let _ = res_writer.writer.flush().await;
                } else {
                    println!("[-] Invalid message received");
                }
            }
            Ok(_) => {
                break Terminate::ClientClosed;
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => continue,
            Err(e) => {
                println!("[-] Failed to read from stream: {e}");
                break Terminate::SocketError;
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
            Ok((stream, _addr)) = listener.accept() => {
                set.spawn(handle_connection(stream, shutdown_rx.resubscribe()));
            }
        }
    }

    set.join_all().await;
}
