use crate::*;
use axum::extract::ws::WebSocket;
use chrono::Utc;
use sled::Db;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

pub fn initialize_mailbox() -> Result<Db, ServerError> {
    sled::open("mailbox")
        .map_err(|_| ServerError::StorageFailed("Failed to initialize database".to_owned()))
}

pub fn retrieve_blobs_for_peer(
    mailbox: &sled::Db,
    recipient_hash: &[u8; 32],
) -> Result<Vec<ServerMessage>, ServerError> {
    mailbox
        .scan_prefix(recipient_hash)
        .map(|entry| -> Result<ServerMessage, ServerError> {
            let (_key, value) = entry.map_err(|e| ServerError::StorageFailed(e.to_string()))?;

            let mail: MailMessage = rmp_serde::from_slice(&value)
                .map_err(|e| ServerError::DeserializationFailed(e.to_string()))?;

            Ok(ServerMessage::PendingBlob {
                blob_id: mail.blob_id,
                blob: mail.blob,
                timestamp: mail.timestamp,
            })
        })
        .collect()
}

pub async fn send_blobs(
    messages: &Vec<ServerMessage>,
    socket: &mut WebSocket,
    state: &Arc<AppState>,
    pub_key: [u8; 32],
) -> Result<(), ServerError> {
    for message in messages.iter() {
        let blob_id_to_verify = match message {
            ServerMessage::PendingBlob { blob_id, .. } => blob_id,
            _ => return Err(ServerError::IncorrectMessage),
        };
        let caller_hash: [u8; 32] = pub_key;
        parse_and_send(&ServerToClient::new(message.to_owned()), socket).await?;
        let data = timeout(Duration::from_secs(3), socket.recv())
            .await
            .map_err(|_| ServerError::WebsocketReceiveFailed)?;
        let response = parse_message_from_client(data)?;
        match response.payload {
            ClientMessage::AckBlob { blob_id } if &blob_id == blob_id_to_verify => {
                delete_blob(state, blob_id, caller_hash)?;
                continue;
            }
            ClientMessage::AckBlob { .. } => return Err(ServerError::BadPacket),
            _ => return Err(ServerError::IncorrectMessage),
        }
    }
    let end_of_blobs = ServerToClient::new(ServerMessage::PendingBlobsEnd);
    parse_and_send(&end_of_blobs, socket).await?;

    Ok(())
}

pub async fn store_blob(
    state: &Arc<AppState>,
    recipient_hash: [u8; 32],
    blob: Vec<u8>,
) -> Result<(), ServerError> {
    let blob_id = Uuid::new_v4().to_string();
    let timestamp = Utc::now().timestamp();

    let entry = MailMessage {
        blob_id: blob_id.clone(),
        blob: blob.clone(),
        timestamp,
    };
    let value = rmp_serde::to_vec(&entry).map_err(|_| ServerError::SerializationFailed)?;

    let mut key = Vec::with_capacity(32 + blob_id.len());
    key.extend_from_slice(&recipient_hash);
    key.extend_from_slice(blob_id.as_bytes());

    state
        .mailbox
        .insert(&key, value)
        .map_err(|e| ServerError::StorageFailed(e.to_string()))?;

    let index_tree = state
        .mailbox
        .open_tree("blob_to_recipient")
        .map_err(|_| ServerError::StorageFailed("failed to open index tree".to_string()))?;
    index_tree
        .insert(blob_id.as_bytes(), &recipient_hash)
        .map_err(|_| ServerError::StorageFailed("failed to insert into index".to_string()))?;

    let push_msg = ServerToClient::new(ServerMessage::PendingBlob {
        blob_id: blob_id.clone(),
        blob,
        timestamp,
    });

    if let Some(conn) = state.connections.get(&recipient_hash) {
        tracing::debug!(
            recipient = %hex::encode(recipient_hash),
            blob_id = %blob_id,
            "recipient online — pushing blob immediately"
        );
        if let Err(e) = conn.value().send(push_msg).await {
            tracing::warn!("failed to push PendingBlob to online recipient: {:?}", e);
        }
    } else {
        tracing::debug!(
            recipient = %hex::encode(recipient_hash),
            blob_id = %blob_id,
            "recipient offline — blob stored in mailbox"
        );
    }

    Ok(())
}

pub fn delete_blob(
    state: &Arc<AppState>,
    blob_id: String,
    caller_hash: [u8; 32],
) -> Result<(), ServerError> {
    let index_tree = state
        .mailbox
        .open_tree("blob_to_recipient")
        .map_err(|_| ServerError::StorageFailed("Failed to open index tree".to_string()))?;
    let main_tree = state.mailbox.clone();
    let recipient_hash_ivec = index_tree
        .get(blob_id.as_bytes())
        .map_err(|_| ServerError::StorageFailed("Failed to fetch index".to_string()))?
        .ok_or(ServerError::BadPacket)?;

    if recipient_hash_ivec.as_ref() != caller_hash.as_slice() {
        return Err(ServerError::InvalidSignature);
    }
    let mut full_key = Vec::with_capacity(32 + blob_id.len());
    full_key.extend_from_slice(&recipient_hash_ivec);
    full_key.extend_from_slice(blob_id.as_bytes());

    main_tree
        .remove(&full_key)
        .map_err(|_| ServerError::StorageFailed("Failed to delete file from db".to_string()))?;
    index_tree
        .remove(blob_id.as_bytes())
        .map_err(|_| ServerError::StorageFailed("Failed to delete file from index".to_owned()))?;
    Ok(())
}
