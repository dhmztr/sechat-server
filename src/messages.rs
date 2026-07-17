use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientToServer {
    pub payload: ClientMessage,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerToClient {
    pub payload: ServerMessage,
    pub timestamp: i64,
}

impl ServerToClient {
    pub fn new(payload: ServerMessage) -> Self {
        let timestamp = Utc::now().timestamp();
        ServerToClient { payload, timestamp }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    Auth {
        pub_key: [u8; 32],
        verif: [u8; 32],
        signature: Vec<u8>,
    },
    Announce {
        token: [u8; 32],
        ip_port: String,
    },
    Unannounce {
        token: [u8; 32],
    },
    SendBlob {
        recipient_hash: [u8; 32],
        blob: Vec<u8>,
    },
    Purge {
        hash_pubkey: [u8; 32],
    },
    AckBlob {
        blob_id: String,
    },
    LookupPeer {
        token: [u8; 32],
    },
    RequestHolePunch {
        token: [u8; 32],
    },
    RequestDenied {
        token: [u8; 32],
    },
    RequestAccepted {
        token: [u8; 32],
    },
    RelayData {
        recipient_hash: [u8; 32],
        payload: Vec<u8>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ServerMessage {
    AuthOk {
        observed_address: String,
    },
    AuthFailed {
        reason: String,
    },
    PendingBlob {
        blob_id: String,
        blob: Vec<u8>,
        timestamp: i64,
    },
    PeerOnline {
        hash: [u8; 32],
        ip_port: String,
    },
    PeerOffline {
        hash: [u8; 32],
    },
    PunchHole {
        token: [u8; 32],
        peer_hash: [u8; 32],
        ip_port: String,
        punchtimestamp: i64,
    },
    Error {
        reason: String,
    },
    PendingBlobsEnd,
    RequestHolePunch {
        pub_key: [u8; 32],
        token: [u8; 32],
    },
    RequestDenied {
        token: [u8; 32],
        pub_key: [u8; 32],
        reason: RequestDeniedReason,
    },
    RelayData {
        sender_hash: [u8; 32],
        payload: Vec<u8>,
    },
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum RequestDeniedReason {
    PeerDeclined,
    Timeout,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct MailMessage {
    pub blob_id: String,
    pub blob: Vec<u8>,
    pub timestamp: i64,
}
