use std::fmt;

#[derive(Debug)]
pub enum ClientErrors {
    ConnectionFailed(String),
    ConnectionClosed,
    ConnectionTimeout,
    WriteFailed(String),
    ReadFailed(String),

    SerializationFailed(String),
    DeserializationFailed(String),
    InvalidMessageFormat,

    AuthFailed(String),
    AuthTimeout,
    UnexpectedHandshakeMessage,

    InvalidSignature,
    InvalidTimestamp,
    ReplayDetected,
    UnknownPeer,
    InvalidPublicKey,

    DecryptionFailed,
    EncryptionFailed,
    KeyDerivationFailed,

    PeerLoadFailed(String),
    PeerSaveFailed(String),
    ChatStorageFailed(String),

    ChannelClosed,
    ServerError(String),
}

impl fmt::Display for ClientErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientErrors::ConnectionFailed(s) => write!(f, "connection failed: {}", s),
            ClientErrors::ConnectionClosed => write!(f, "connection closed"),
            ClientErrors::ConnectionTimeout => write!(f, "connection timeout"),
            ClientErrors::WriteFailed(s) => write!(f, "write failed: {}", s),
            ClientErrors::ReadFailed(s) => write!(f, "read failed: {}", s),
            ClientErrors::SerializationFailed(s) => write!(f, "serialization failed: {}", s),
            ClientErrors::DeserializationFailed(s) => write!(f, "deserialization failed: {}", s),
            ClientErrors::InvalidMessageFormat => write!(f, "invalid message format"),
            ClientErrors::AuthFailed(s) => write!(f, "auth failed: {}", s),
            ClientErrors::AuthTimeout => write!(f, "auth timeout"),
            ClientErrors::UnexpectedHandshakeMessage => write!(f, "unexpected handshake message"),
            ClientErrors::InvalidSignature => write!(f, "invalid signature"),
            ClientErrors::InvalidTimestamp => write!(f, "invalid timestamp"),
            ClientErrors::ReplayDetected => write!(f, "replay attack detected"),
            ClientErrors::UnknownPeer => write!(f, "unknown peer"),
            ClientErrors::InvalidPublicKey => write!(f, "invalid public key"),
            ClientErrors::DecryptionFailed => write!(f, "decryption failed"),
            ClientErrors::EncryptionFailed => write!(f, "encryption failed"),
            ClientErrors::KeyDerivationFailed => write!(f, "key derivation failed"),
            ClientErrors::PeerLoadFailed(s) => write!(f, "peer load failed: {}", s),
            ClientErrors::PeerSaveFailed(s) => write!(f, "peer save failed: {}", s),
            ClientErrors::ChatStorageFailed(s) => write!(f, "chat storage failed: {}", s),
            ClientErrors::ChannelClosed => write!(f, "internal channel closed"),
            ClientErrors::ServerError(s) => write!(f, "server reported error: {}", s),
        }
    }
}

impl std::error::Error for ClientErrors {}

#[derive(Debug)]
pub enum ServerError {
    BadPacket,
    MpscSendFailed,
    IncorrectMessage,
    SerializationFailed,
    DeserializationFailed(String),
    StorageFailed(String),
    WebsocketSendFailed,
    WebsocketReceiveFailed,
    InvalidPublicKey,
    InvalidSignature,
    TimestampOutOfRange,
    HandshakeFailed,
    PresenceFailed,
}
