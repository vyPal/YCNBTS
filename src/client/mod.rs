use std::sync::Arc;

use clap::Parser;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::Mutex,
};

pub struct Client {
    readonly_half: Arc<Mutex<OwnedReadHalf>>,
    writeable_half: Arc<Mutex<OwnedWriteHalf>>,
}

impl Client {
    pub async fn new(host: String, port: u16) -> Self {
        let stream = tokio::net::TcpStream::connect(format!("{}:{}", host, port))
            .await
            .unwrap();
        let (readable_half, writeable_half) = stream.into_split();

        Client {
            readonly_half: Arc::new(Mutex::new(readable_half)),
            writeable_half: Arc::new(Mutex::new(writeable_half)),
        }
    }

    pub async fn send_message(&self, message: crate::shared::messages::ClientBoundMessage) {
        let mut buffer = Vec::new();
        bincode::serialize_into(&mut buffer, &message).unwrap();

        let mut buffer_with_length = Vec::new();
        bincode::serialize_into(&mut buffer_with_length, &(buffer.len() as u64)).unwrap();
        buffer_with_length.extend(buffer);

        self.writeable_half
            .lock()
            .await
            .write_all(&buffer_with_length)
            .await
            .unwrap();
    }

    pub async fn handle(&self) {
        loop {
            let mut length_buf = [0u8; 8];
            if self
                .readonly_half
                .lock()
                .await
                .read_exact(&mut length_buf)
                .await
                .is_err()
            {
                break;
            }

            let message_len: u64 = match bincode::deserialize_from(&length_buf[..]) {
                Ok(len) => len,
                Err(_) => break,
            };

            let mut buffer = vec![0u8; message_len as usize];
            if self
                .readonly_half
                .lock()
                .await
                .read_exact(&mut buffer)
                .await
                .is_err()
            {
                break;
            }

            match bincode::deserialize_from::<&[u8], crate::shared::messages::ClientBoundMessage>(
                &buffer[..],
            ) {
                Ok(message) => {
                    println!("Received message: {:?}", message);
                }
                Err(e) => {
                    eprintln!("Failed to deserialize message: {}", e);
                }
            };
        }
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    /// Address to bind to
    #[arg(short, long, default_value = "127.0.0.1")]
    pub address: String,

    /// Port to bind to
    #[arg(short, long, default_value_t = 8080)]
    pub port: u16,
}
