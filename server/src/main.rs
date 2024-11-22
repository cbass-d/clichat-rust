use std::io::{prelude::*, BufReader, ErrorKind};
use std::time::Duration;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, Interest},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    signal::ctrl_c,
    task::JoinSet,
};

struct RequestHandler {
    reader: OwnedReadHalf,
}

struct ResponseWriter {
    writer: OwnedWriteHalf,
}

type ClientConnection = (RequestHandler, ResponseWriter);

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

async fn handle_connection(stream: TcpStream) -> Terminate {
    let (req_handler, mut res_writer) = split_stream(stream);
    let mut buf = Vec::with_capacity(4096);

    loop {
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
    println!("(+) Starting listener...");
    let listener = match startup_server().await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("[-] Failed to start server: {e}");
            panic!();
        }
    };
    let mut set: JoinSet<Terminate> = JoinSet::new();
    println!("(+) Server started...\n(+) Listening for connections...");
    let mut ticker = tokio::time::interval(Duration::from_millis(250));

    loop {
        tokio::select! {
            _ = ticker.tick() => {},
            Ok(_) = ctrl_c() => {
                break;
            },
            Ok((stream, _addr)) = listener.accept() => {
                set.spawn(handle_connection(stream));
            }
        }
    }

    set.join_all().await;
}
