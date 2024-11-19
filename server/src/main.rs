use std::{
    io::{prelude::*, BufReader},
    net::{TcpListener, TcpStream},
    thread,
};

struct Session {
    id: i32,
    nick: String,
    active: bool,
}

struct ClientThread {
    id: i32,
}

fn startup_server() -> Result<TcpListener, std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:6667")?;
    return Ok(listener);
}

fn handle_incoming(mut stream: &TcpStream) {
    let mut buf_reader = BufReader::new(&mut stream);
    let message = buf_reader.fill_buf().unwrap();

    if message.len() != 0 {
        println!("{message:?}");
    }

    let len = message.len();
    buf_reader.consume(len);
}

fn main() {
    println!("(+) Starting listener...");
    let listener: TcpListener = match startup_server() {
        Ok(listener) => listener,
        Err(e) => panic!("Failed to start up listener: {e}"),
    };

    println!("(+) Server started...\n(+) Listening for connections...");

    let mut num_connections: u64 = 0;
    let mut active_threads: u64 = 0;
    let mut thread_handles = vec![];
    let mut client_sessions: Vec<Session> = vec![];
    for stream in listener.incoming() {
        // Get and store needed info about stream
        let mut stream = stream.unwrap();

        let handle = thread::spawn(move || {
            let mut stream_clone = stream.try_clone().unwrap();
            let buf_reader = BufReader::new(&mut stream_clone);
            let mut buf = String::new();

            for line in buf_reader.lines() {
                buf = line.unwrap();
                println!("From client: {buf}");

                stream.write_all(b"Response from server\n").unwrap();
                stream.flush().unwrap();
            }
        });

        thread_handles.push(handle);
        active_threads += 1;
        num_connections += 1;
    }

    for handle in thread_handles {
        handle.join().unwrap();
    }
}
