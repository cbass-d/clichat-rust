use common::message::Message;
use std::io;
use tokio::{
    io::AsyncWriteExt,
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
};

pub struct ClientConnection {
    buf: Vec<u8>,
    stream_out: OwnedWriteHalf,
    stream_in: OwnedReadHalf,
}

impl ClientConnection {
    pub fn new(stream: TcpStream) -> Self {
        let (stream_in, stream_out) = stream.into_split();
        Self {
            buf: Vec::with_capacity(2048),
            stream_out,
            stream_in,
        }
    }

    pub async fn write(&mut self, message: Message) -> Result<(), io::Error> {
        let bytes_to_write = message.to_bytes();

        self.stream_out.write_all(&bytes_to_write).await?;

        Ok(())
    }

    pub async fn readable(&self) -> Result<(), io::Error> {
        self.stream_in.readable().await
    }

    pub async fn read(&mut self) -> Result<Option<Message>, io::Error> {
        match self.stream_in.try_read_buf(&mut self.buf) {
            Ok(len) if len > 0 => {
                let bytes = &self.buf[0..len];
                let message = Message::from_bytes(bytes.to_vec());
                self.buf.clear();

                if let Ok(message) = message {
                    Ok(Some(message))
                } else {
                    Ok(None)
                }
            }
            Ok(_) => Err(io::ErrorKind::BrokenPipe.into()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Err(e),
            Err(e) => Err(e),
        }
    }
}
