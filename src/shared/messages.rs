use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type ClientDescription = (String, Uuid);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientBoundMessage {
    ClientList(Vec<ClientDescription>),
    NewClient(ClientDescription),
    ConnectionRequest(ClientDescription),
    ConnectionResponse(bool),
    Message(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerBoundMessage {
    Advertise(String),
    ConnectionRequest(ClientDescription),
    ConnectionResponse(bool),
    Message(String),
}
