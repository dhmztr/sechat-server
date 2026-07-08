use crate::*;
use std::sync::Arc;
use tokio::sync::mpsc;

pub fn retrieve_connection(
    id: [u8; 32],
    state: &Arc<AppState>,
) -> Option<mpsc::Sender<ServerToClient>> {
    state.connections.get(&id).map(|r| r.clone())
}

pub fn insert_connection(
    id: [u8; 32],
    tx: mpsc::Sender<ServerToClient>,
    state: &Arc<AppState>,
) -> Result<(), ServerError> {
    state.connections.insert(id, tx);
    Ok(())
}

pub async fn cleanup_client(id: [u8; 32], state: &Arc<AppState>) {
    state.connections.remove(&id);

    let mut affected_tokens: Vec<[u8; 32]> = Vec::new();

    state.presence.retain(|token, entries| {
        let was_present = entries.iter().any(|e| e.pub_key == id);
        if was_present {
            affected_tokens.push(*token);
        }
        entries.retain(|e| e.pub_key != id);
        !entries.is_empty()
    });

    for token in affected_tokens {
        if let Some(entries) = state.presence.get(&token) {
            for entry in entries.iter() {
                if let Some(sender) = state.connections.get(&entry.pub_key) {
                    let msg = ServerToClient::new(ServerMessage::PeerOffline { hash: id });
                    let _ = sender.send(msg).await;
                }
            }
        }
    }
}
