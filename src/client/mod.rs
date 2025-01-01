use std::sync::Arc;

use clap::Parser;
use inquire::{Confirm, Select, Text};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::Mutex,
};
use uuid::Uuid;

use crate::shared::messages::{ClientBoundMessage, ClientDescription, ServerBoundMessage};

pub struct Client {
    readonly_half: Arc<Mutex<OwnedReadHalf>>,
    writeable_half: Arc<Mutex<OwnedWriteHalf>>,
    peer_list: Arc<Mutex<Vec<ClientDescription>>>,
    uuid: Arc<Mutex<Option<Uuid>>>,
    connection_requests: Arc<Mutex<Vec<ClientDescription>>>,
    open_connections: Arc<Mutex<Vec<Uuid>>>,
    current_channel: Arc<Mutex<Option<Uuid>>>,
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
            peer_list: Arc::new(Mutex::new(Vec::new())),
            uuid: Arc::new(Mutex::new(None)),
            connection_requests: Arc::new(Mutex::new(Vec::new())),
            open_connections: Arc::new(Mutex::new(Vec::new())),
            current_channel: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn send_message(&self, message: crate::shared::messages::ServerBoundMessage) {
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

            match bincode::deserialize_from::<&[u8], ClientBoundMessage>(&buffer[..]) {
                Ok(message) => match message {
                    ClientBoundMessage::SetUuid(uuid) => {
                        *self.uuid.lock().await = Some(uuid);
                    }
                    ClientBoundMessage::ClientList(client_description) => {
                        *self.peer_list.lock().await = client_description;
                    }
                    ClientBoundMessage::NewClient(client_description) => {
                        self.peer_list.lock().await.push(client_description);
                    }
                    ClientBoundMessage::ClientDisconnected(uuid) => {
                        let mut peer_list = self.peer_list.lock().await;
                        peer_list.retain(|(_, id)| *id != uuid);
                    }
                    ClientBoundMessage::ConnectionRequest(client_description) => {
                        let mut connection_requests = self.connection_requests.lock().await;
                        if connection_requests
                            .iter()
                            .find(|(_, id)| *id == client_description.1)
                            .is_none()
                        {
                            connection_requests.push(client_description);
                            println!("\n\r\n You have a new connection request. Type 'accept' to view and accept it.\n\r");
                        }
                    }
                    ClientBoundMessage::ConnectionResponse(client_description, accepted) => {
                        if accepted {
                            let mut open_connections = self.open_connections.lock().await;
                            open_connections.push(client_description.1);
                            println!("\n\r\n Connection accepted.Type 'open' again to choose channel.\n\r");
                        } else {
                            println!("\n\r\n Connection rejected.\n\r");
                        }
                    }
                    ClientBoundMessage::Message(client_description, message) => {
                        let name = self
                            .peer_list
                            .lock()
                            .await
                            .iter()
                            .find(|(_, id)| *id == client_description.1)
                            .map(|(name, _)| name.clone())
                            .unwrap_or("Unknown".to_string());
                        println!("\n\r\n{}: {}\n\r", name, message);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to deserialize message: {}", e);
                }
            };
        }
    }
}

impl Client {
    pub async fn run_ui(&self) {
        let set_friendly_name = Confirm::new("Set friendly name?")
            .with_default(true)
            .prompt()
            .unwrap();
        if set_friendly_name {
            let friendly_name = Text::new("Friendly name")
                .with_placeholder("Enter a name that other clients will see")
                .with_default("Anonymous Turtle 🐢")
                .prompt()
                .unwrap();
            let message = crate::shared::messages::ServerBoundMessage::Advertise(friendly_name);
            self.send_message(message).await;
        } else {
            println!(
                "\n\n No friendly name set. Your uuid will not be displayed to other clients.\n"
            );
            print!("Anyone who wants to connect to you will need to know your uuid. Type 'uuid' to view it.\n\n");
        }
        loop {
            println!();
            let action = Text::new("Action")
                .with_placeholder("Type 'exit' to exit or 'help' to view available actions")
                .prompt()
                .unwrap();
            match action.as_str() {
                "exit" => break,
                "help" => Self::display_help().await,
                "uuid" => self.display_uuid().await,
                "list" => self.list_peers().await,
                "open" => self.open_connection(None).await,
                "accept" => self.accept_connection().await,
                "" => {}
                _ => {
                    if action.starts_with("open") {
                        let uuid = action
                            .split_whitespace()
                            .nth(1)
                            .map(|uuid| Uuid::parse_str(uuid).unwrap());
                        self.open_connection(uuid).await;
                    } else if action.starts_with("send") {
                        let message = action.splitn(2, ' ').nth(1).unwrap_or("").to_string();
                        self.ui_send_message(message).await;
                    } else {
                        println!("Unknown action: {}", action);
                    }
                }
            }
        }
    }

    async fn display_help() {
        println!();
        println!("Available actions:");
        println!("exit: Exit the program");
        println!("help: Display this help message");
        println!("uuid: Display your uuid");
        println!("list: List available peers");
        println!("open (uuid?): Open a connection to a peer");
        println!("close: Close a connection to a peer");
        println!("accept: View pending connection requests");
        println!("send <message>: Send a message to current channel");
    }

    async fn display_uuid(&self) {
        let uuid = self.uuid.lock().await;
        println!();
        println!("Your uuid is: {}", uuid.unwrap());
    }

    async fn list_peers(&self) {
        let peer_list = self.peer_list.lock().await;
        println!();
        println!("Available peers:");
        for (name, uuid) in peer_list.iter() {
            println!("{}: {}", uuid, name);
        }
    }

    async fn open_connection(&self, uuid: Option<Uuid>) {
        let open_connections = self.open_connections.lock().await;
        let mut current_channel = self.current_channel.lock().await;

        if let Some(uuid) = uuid {
            if open_connections.contains(&uuid) {
                if *current_channel == Some(uuid) {
                    println!("\n\r\n You are already connected to this channel.\n\r");
                } else {
                    println!("\n\r\n You are now connected to this channel.\n\r");
                    *current_channel = Some(uuid);
                }
                return;
            }

            let message = ServerBoundMessage::ConnectionRequest(("".to_string(), uuid));
            self.send_message(message).await;
            return;
        }

        let peer_list = self.peer_list.lock().await;
        let options = peer_list
            .iter()
            .map(|(name, uuid)| {
                if open_connections.contains(uuid) {
                    format!("{}: {} (Connected)", uuid, name)
                } else {
                    format!("{}: {}", uuid, name)
                }
            })
            .collect::<Vec<_>>();

        let selection = Select::new("Select a peer", options).prompt();
        if selection.is_err() {
            return;
        }
        let selection = selection.unwrap();

        let selected_peer = peer_list
            .iter()
            .find(|(name, uuid)| {
                format!("{}: {}", uuid, name) == selection
                    || format!("{}: {} (Connected)", uuid, name) == selection
            })
            .unwrap();

        if open_connections.contains(&selected_peer.1) {
            if *current_channel == Some(selected_peer.1) {
                println!("\n\r\n You are already connected to this channel.\n\r");
            } else {
                println!("\n\r\n You are now connected to this channel.\n\r");
                *current_channel = Some(selected_peer.1);
            }
            return;
        }

        let message = ServerBoundMessage::ConnectionRequest(selected_peer.clone());
        self.send_message(message).await;
    }

    async fn accept_connection(&self) {
        let connection_requests = self.connection_requests.lock().await;
        let options = connection_requests
            .iter()
            .map(|(name, uuid)| format!("{}: {}", uuid, name))
            .collect::<Vec<_>>();

        let selection = Select::new("Select a peer", options).prompt();
        if selection.is_err() {
            return;
        }
        let selection = selection.unwrap();

        let selected_peer = connection_requests
            .iter()
            .find(|(name, uuid)| format!("{}: {}", uuid, name) == selection);

        let message = ServerBoundMessage::ConnectionResponse(selected_peer.unwrap().clone(), true);
        self.send_message(message).await;
    }

    async fn ui_send_message(&self, message: String) {
        let current_channel = self.current_channel.lock().await;
        if let Some(current_channel) = *current_channel {
            let message = ServerBoundMessage::Message(("".to_string(), current_channel), message);
            self.send_message(message).await;
        } else {
            println!("\n\r\n You are not connected to a channel.\n\r");
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
