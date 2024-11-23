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

struct RequestHandler {
    reader: OwnedReadHalf,
}

struct ResponseWriter {
    writer: OwnedWriteHalf,
}

type ClientConnection = (RequestHandler, ResponseWriter);

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
                let s = String::from_utf8(buf[0..len].to_vec()).unwrap();
                println!("{s}");
                let _ = res_writer.writer.write_all(&buf[0..len]).await;
                let _ = res_writer.writer.flush().await;
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
