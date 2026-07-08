use crate::*;
use chrono::Utc;
use std::sync::Arc;

pub struct PresenceEntry {
    pub pub_key: [u8; 32],
    pub ip_port: String,
    pub timestamp: i64,
}

pub async fn handle_presence(
    token: [u8; 32],
    ip_port: String,
    timestamp: i64,
    pub_key: &[u8; 32],
    state: &Arc<AppState>,
) -> Result<Vec<([u8; 32], String)>, ServerError> {
    {
        let mut entries = state.presence.entry(token).or_insert_with(Vec::new);
        entries.retain(|e| &e.pub_key != pub_key);
        entries.push(PresenceEntry {
            pub_key: *pub_key,
            ip_port,
            timestamp,
        });
    }
    retrieve_peers(state, token, pub_key)
}

pub fn retrieve_peers(
    state: &Arc<AppState>,
    token: [u8; 32],
    pub_key: &[u8; 32],
) -> Result<Vec<([u8; 32], String)>, ServerError> {
    let entries = state.presence.entry(token).or_insert_with(Vec::new);
    Ok(entries
        .iter()
        .filter(|&item| &item.pub_key != pub_key)
        .map(|item| (item.pub_key, item.ip_port.clone()))
        .collect::<Vec<([u8; 32], String)>>())
}

pub async fn presence_cleanup_loop(state: &Arc<AppState>) {
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(15));
    loop {
        ticker.tick().await;
        let cutoff = Utc::now().timestamp() - 30;
        state.presence.retain(|_token, entries| {
            entries.retain(|e| e.timestamp > cutoff);
            !entries.is_empty()
        });
    }
}
