use std::{collections::HashMap, sync::Arc};

use bincode::deserialize_from;
use clap::Parser;
use client::Client;
use tokio::{io::AsyncReadExt, net::TcpListener, sync::Mutex};

use crate::shared::messages::{ClientBoundMessage, ClientDescription, ServerBoundMessage};

mod client;

pub struct Server {
    clients: HashMap<uuid::Uuid, Client>,
    listener: TcpListener,
}

impl Server {
    pub async fn new(address: String, port: u16) -> Self {
        let listener = TcpListener::bind(format!("{}:{}", address, port))
            .await
            .unwrap();
        let clients = HashMap::new();

        Server { clients, listener }
    }

    pub async fn run(&mut self) {
        loop {
            let (stream, _) = self.listener.accept().await.unwrap();

            let uuid = uuid::Uuid::new_v4();
            let friendly_name = format!("Client-{}", uuid);

            let (readable_half, writeable_half) = stream.into_split();

            let client = Client {
                readonly_half: Arc::new(Mutex::new(readable_half)),
                writeable_half: Arc::new(Mutex::new(writeable_half)),
                friendly_name: friendly_name.clone(),
                uuid,
            };
            let clients_before = self.clients.clone();
            self.clients.insert(uuid, client.clone());

            let client_clone = client.clone();
            tokio::spawn(async move {
                let client_clone = client_clone.clone();
                loop {
                    let mut length_buf = [0u8; 8];
                    if client_clone
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
                    if client_clone
                        .readonly_half
                        .lock()
                        .await
                        .read_exact(&mut buffer)
                        .await
                        .is_err()
                    {
                        break;
                    }

                    match bincode::deserialize_from::<
                        &[u8],
                        crate::shared::messages::ServerBoundMessage,
                    >(&buffer[..])
                    {
                        Ok(message) => {
                            println!("Received message: {:?}", message);
                        }
                        Err(e) => {
                            eprintln!("Failed to deserialize message: {}", e);
                        }
                    };
                }
            });

            println!("New client connected: {}", friendly_name);

            let client_descriptions: Vec<ClientDescription> = clients_before
                .iter()
                .map(|(uuid, client)| (client.friendly_name.clone(), *uuid))
                .collect();

            let message = ClientBoundMessage::ClientList(client_descriptions);
            client.send_message(message).await;
        }
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    /// Address to bind to
    #[arg(short, long, default_value = "0.0.0.0")]
    pub address: String,

    /// Port to bind to
    #[arg(short, long, default_value_t = 8080)]
    pub port: u16,
}
