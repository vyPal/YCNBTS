use std::{collections::HashMap, sync::Arc};

use bincode::deserialize_from;
use clap::Parser;
use client::Client;
use tokio::{io::AsyncReadExt, net::TcpListener, sync::Mutex};

use crate::shared::messages::{ClientBoundMessage, ClientDescription, ServerBoundMessage};

mod client;

pub struct Server {
    clients: Arc<Mutex<HashMap<uuid::Uuid, Client>>>,
    listener: TcpListener,
}

impl Server {
    pub async fn new(address: String, port: u16) -> Self {
        let listener = TcpListener::bind(format!("{}:{}", address, port))
            .await
            .unwrap();
        let clients = Arc::new(Mutex::new(HashMap::new()));

        Server { clients, listener }
    }

    pub async fn run(&mut self) {
        loop {
            let (stream, _) = self.listener.accept().await.unwrap();

            let uuid = uuid::Uuid::new_v4();

            let (readable_half, writeable_half) = stream.into_split();

            let client = Client {
                readonly_half: Arc::new(Mutex::new(readable_half)),
                writeable_half: Arc::new(Mutex::new(writeable_half)),
                friendly_name: Arc::new(std::sync::Mutex::new(None)),
                uuid,
            };
            self.clients.lock().await.insert(uuid, client.clone());

            let client_clone = client.clone();
            let clients_clone = self.clients.clone();
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
                            match message {
                                ServerBoundMessage::Advertise(name) => {
                                    *client_clone.friendly_name.lock().unwrap() = Some(name.clone());
                                    let message = ClientBoundMessage::NewClient((name, client_clone.uuid));
                                    for client in clients_clone.lock().await.values() {
                                        client.send_message(message.clone()).await;
                                    }
                                },
                                ServerBoundMessage::ConnectionRequest(client_description) => {
                                    let clients_lock = clients_clone.lock().await;
                                    let target_client = clients_lock.get(&client_description.1);
                                    if let Some(target_client) = target_client {
                                        let message = ClientBoundMessage::ConnectionRequest((client_clone.friendly_name.lock().unwrap().clone().unwrap_or("".to_string()), client_clone.uuid));
                                        target_client.send_message(message).await;
                                    }
                                },
                                ServerBoundMessage::ConnectionResponse(client_description, response) => {
                                    let clients_lock = clients_clone.lock().await;
                                    let target_client = clients_lock.get(&client_description.1);
                                    if let Some(target_client) = target_client {
                                        let message = ClientBoundMessage::ConnectionResponse((client_clone.friendly_name.lock().unwrap().clone().unwrap_or("".to_string()), client_clone.uuid), response);
                                        target_client.send_message(message).await;
                                    }
                                },
                                _ => {
                                    eprintln!("Unhandled message: {:?}", message);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to deserialize message: {}", e);
                        }
                    };
                }
                println!("Client disconnected: {}", client_clone.uuid);
                clients_clone.lock().await.remove(&client_clone.uuid);
                let message = ClientBoundMessage::ClientDisconnected(client_clone.uuid);
                for client in clients_clone.lock().await.values() {
                    client.send_message(message.clone()).await;
                }
            });

            println!("New client connected: {}", uuid);

            let uuid_message = ClientBoundMessage::SetUuid(uuid);
            client.send_message(uuid_message).await;

            let client_descriptions: Vec<ClientDescription> = self.clients.lock().await
                .iter()
                .filter(|(_, c)| c.friendly_name.lock().unwrap().is_some())
                .map(|(uuid, client)| (client.friendly_name.lock().unwrap().clone().unwrap(), *uuid))
                .collect();

            println!("Describing clients: {:?}", client_descriptions);

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
