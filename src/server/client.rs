use std::sync::Arc;

use tokio::{
    io::AsyncWriteExt,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::Mutex,
};

#[derive(Clone)]
pub struct Client {
    pub readonly_half: Arc<Mutex<OwnedReadHalf>>,
    pub writeable_half: Arc<Mutex<OwnedWriteHalf>>,
    pub friendly_name: Arc<std::sync::Mutex<Option<String>>>,
    pub uuid: uuid::Uuid,
}

impl Client {
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
}
