use crate::*;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;

#[derive(Serialize, Deserialize)]
pub struct StunPing {
    pub pub_key: [u8; 32],
    pub verif: [u8; 32],
    pub timestamp: i64,
    pub signature: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct StunReply {
    pub observed: String,
}

pub async fn run_stun_server(port: u16) -> Result<(), ServerError> {
    let bind = format!("0.0.0.0:{port}");
    let socket = UdpSocket::bind(&bind)
        .await
        .map_err(|e| ServerError::StorageFailed(format!("stun bind failed: {e}")))?;
    tracing::info!("STUN UDP responder listening on {bind}");

    let mut buf = [0u8; 1024];
    loop {
        let (len, src) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ping: StunPing = match rmp_serde::from_slice(&buf[..len]) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if verify_message(&ping.pub_key, &ping.verif, ping.signature, ping.timestamp).is_err() {
            continue;
        }
        let reply = StunReply {
            observed: src.to_string(),
        };
        if let Ok(bytes) = rmp_serde::to_vec(&reply) {
            let _ = socket.send_to(&bytes, src).await;
            tracing::debug!(%src, "stun: echoed observed address");
        }
    }
}
