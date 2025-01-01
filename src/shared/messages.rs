use rsa::RsaPublicKey;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type ClientDescription = (String, Uuid);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientBoundMessage {
    SetUuid(Uuid),
    ClientList(Vec<ClientDescription>),
    NewClient(ClientDescription),
    ClientDisconnected(Uuid),
    ConnectionRequest(ClientDescription, RsaPublicKey),
    ConnectionResponse(ClientDescription, RsaPublicKey),
    Message(ClientDescription, (Vec<u8>, Vec<u8>, Vec<u8>)),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerBoundMessage {
    Advertise(String),
    ConnectionRequest(ClientDescription, RsaPublicKey),
    ConnectionResponse(ClientDescription, RsaPublicKey),
    Message(ClientDescription, (Vec<u8>, Vec<u8>, Vec<u8>)),
}
