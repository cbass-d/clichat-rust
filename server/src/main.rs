use std::io::{prelude::*, BufReader, ErrorKind};
use std::time::Duration;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, Interest},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
};

struct RequestHandler {
    reader: OwnedReadHalf,
}

struct ResponseWriter {
    writer: OwnedWriteHalf,
}

type ClientConnection = (RequestHandler, ResponseWriter);

fn split_stream(stream: TcpStream) -> ClientConnection {
    let (reader, writer) = stream.into_split();

    (RequestHandler { reader }, ResponseWriter { writer })
}

async fn startup_server() -> Result<TcpListener, std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:6667").await?;
    return Ok(listener);
}

async fn handle_connection(client_connection: &mut ClientConnection) {
    let (req_handler, res_writer) = client_connection;
    let mut buf = Vec::with_capacity(4096);

    loop {
        match req_handler.reader.try_read_buf(&mut buf) {
            Ok(len) if len > 0 => {
                let s = String::from_utf8(buf[0..len].to_vec()).unwrap();
                println!("{s}");
                let w = res_writer.writer.write_all(&buf[0..len]).await;
                let l = res_writer.writer.flush().await;
            }
            Ok(_) => {
                break;
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => continue,
            Err(e) => {
                println!("[-] Failed to read from stream: {e}");
                break;
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

    println!("(+) Server started...\n(+) Listening for connections...");

    let (stream, _addr) = listener.accept().await.unwrap();
    let mut client_connection = split_stream(stream);

    let task = tokio::spawn(async move {
        handle_connection(&mut client_connection).await;
    });

    let _ = tokio::join!(task);
}
