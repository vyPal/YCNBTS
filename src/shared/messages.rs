use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type ClientDescription = (String, Uuid);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientBoundMessage {
    SetUuid(Uuid),
    ClientList(Vec<ClientDescription>),
    NewClient(ClientDescription),
    ClientDisconnected(Uuid),
    ConnectionRequest(ClientDescription),
    ConnectionResponse(ClientDescription, bool),
    Message(ClientDescription, String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerBoundMessage {
    Advertise(String),
    ConnectionRequest(ClientDescription),
    ConnectionResponse(ClientDescription, bool),
    Message(ClientDescription, String),
}
