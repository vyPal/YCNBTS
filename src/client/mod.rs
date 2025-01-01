use std::{collections::HashMap, sync::Arc};

use aes_gcm::{aead::Aead, AeadCore, Aes256Gcm, Key, KeyInit};
use clap::Parser;
use inquire::{Confirm, Select, Text};
use rand::{rngs::OsRng, RngCore};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
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
    connection_requests: Arc<Mutex<HashMap<ClientDescription, RsaPublicKey>>>,
    open_connections: Arc<Mutex<HashMap<Uuid, RsaPublicKey>>>,
    current_channel: Arc<Mutex<Option<Uuid>>>,
    private_key: Arc<RsaPrivateKey>,
    public_key: Arc<RsaPublicKey>,
}

impl Client {
    pub async fn new(host: String, port: u16) -> Self {
        let stream = tokio::net::TcpStream::connect(format!("{}:{}", host, port))
            .await
            .unwrap();
        let (readable_half, writeable_half) = stream.into_split();

        let mut rng = rand::thread_rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let public_key = RsaPublicKey::from(&private_key);

        Client {
            readonly_half: Arc::new(Mutex::new(readable_half)),
            writeable_half: Arc::new(Mutex::new(writeable_half)),
            peer_list: Arc::new(Mutex::new(Vec::new())),
            uuid: Arc::new(Mutex::new(None)),
            connection_requests: Arc::new(Mutex::new(HashMap::new())),
            open_connections: Arc::new(Mutex::new(HashMap::new())),
            current_channel: Arc::new(Mutex::new(None)),
            private_key: Arc::new(private_key),
            public_key: Arc::new(public_key),
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
                    ClientBoundMessage::ConnectionRequest(client_description, public_key) => {
                        let mut connection_requests = self.connection_requests.lock().await;
                        if !connection_requests
                            .iter()
                            .any(|((_, id), _)| *id == client_description.1)
                        {
                            connection_requests.insert(client_description, public_key);
                            println!("\n\r\n You have a new connection request. Type 'accept' to view and accept it.\n\r");
                        }
                    }
                    ClientBoundMessage::ConnectionResponse(client_description, public_key) => {
                        let mut open_connections = self.open_connections.lock().await;
                        open_connections.insert(client_description.1, public_key);
                        println!("\n\r\n Connection accepted.Type 'open' again to choose channel.\n\r");
                    }
                    ClientBoundMessage::Message(client_description, (encrypted_key, nonce, ciphertext)) => {
                        let name = self
                            .peer_list
                            .lock()
                            .await
                            .iter()
                            .find(|(_, id)| *id == client_description.1)
                            .map(|(name, _)| name.clone())
                            .unwrap_or("Unknown".to_string());

                        let session_key = self.private_key.decrypt(Pkcs1v15Encrypt, &encrypted_key).unwrap();

                        let key = Key::<Aes256Gcm>::from_slice(&session_key);
                        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();

                        let message = cipher.decrypt((&*nonce).into(), &*ciphertext).unwrap();
                        let message = String::from_utf8(message).unwrap();

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
                        let message = action
                            .split_once(' ')
                            .map(|x| x.1)
                            .unwrap_or("")
                            .to_string();
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
            if open_connections.contains_key(&uuid) {
                if *current_channel == Some(uuid) {
                    println!("\n\r\n You are already connected to this channel.\n\r");
                } else {
                    println!("\n\r\n You are now connected to this channel.\n\r");
                    *current_channel = Some(uuid);
                }
                return;
            }

            let message = ServerBoundMessage::ConnectionRequest(("".to_string(), uuid), (*self.public_key).clone());
            self.send_message(message).await;
            return;
        }

        let peer_list = self.peer_list.lock().await;
        let options = peer_list
            .iter()
            .map(|(name, uuid)| {
                if open_connections.contains_key(uuid) {
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

        if open_connections.contains_key(&selected_peer.1) {
            if *current_channel == Some(selected_peer.1) {
                println!("\n\r\n You are already connected to this channel.\n\r");
            } else {
                println!("\n\r\n You are now connected to this channel.\n\r");
                *current_channel = Some(selected_peer.1);
            }
            return;
        }

        let message = ServerBoundMessage::ConnectionRequest(selected_peer.clone(), (*self.public_key).clone());
        self.send_message(message).await;
    }

    async fn accept_connection(&self) {
        let connection_requests = self.connection_requests.lock().await;
        let options = connection_requests
            .iter()
            .map(|((name, uuid), _)| format!("{}: {}", uuid, name))
            .collect::<Vec<_>>();

        let selection = Select::new("Select a peer", options).prompt();
        if selection.is_err() {
            return;
        }
        let selection = selection.unwrap();

        let selected_peer = connection_requests
            .iter()
            .find(|((name, uuid), _)| format!("{}: {}", uuid, name) == selection);

        if selected_peer.is_none() {
            println!("\n\r\n Invalid selection.\n\r");
            return;
        }

        self.open_connections.lock().await.insert(selected_peer.unwrap().0 .1, selected_peer.unwrap().1.clone());

        let message = ServerBoundMessage::ConnectionResponse(selected_peer.unwrap().0.clone(), (*self.public_key).clone());
        self.send_message(message).await;
    }

    async fn ui_send_message(&self, message: String) {
        let current_channel = self.current_channel.lock().await;
        if let Some(current_channel) = *current_channel {
            let mut rng = OsRng;
            let mut session_key = [0u8; 32]; 
            rng.fill_bytes(&mut session_key);

            let open_connections = self.open_connections.lock().await;
            let remote_public_key = open_connections.get(&current_channel).unwrap();
            let encrypted_key = remote_public_key.encrypt(&mut rng, Pkcs1v15Encrypt, &session_key).unwrap();

            let nonce = Aes256Gcm::generate_nonce(&mut rng);

            let key = Key::<Aes256Gcm>::from_slice(&session_key);
            let cipher = Aes256Gcm::new_from_slice(&key).unwrap();

            let ciphertext = cipher.encrypt(&nonce, message.as_bytes()).unwrap();

            let message = ServerBoundMessage::Message(("".to_string(), current_channel), (encrypted_key, nonce.to_vec(), ciphertext));
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
