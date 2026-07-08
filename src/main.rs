use axum::{Router, routing::any};
use axum_server::tls_rustls::RustlsConfig;
use dashmap::DashMap;
use ed25519_dalek::*;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

mod connection;
mod crypto;
mod errors;
mod mailbox;
mod messages;
mod presence;
mod stun;
mod websocket;
mod wire;
use connection::*;
use crypto::*;
use errors::*;
use mailbox::*;
use messages::*;
use presence::*;
use websocket::*;
use wire::*;

pub struct AppState {
    pub presence: DashMap<[u8; 32], Vec<PresenceEntry>>,
    pub connections: DashMap<[u8; 32], mpsc::Sender<ServerToClient>>,
    pub mailbox: sled::Db,
    pub pendingp2ps: DashMap<[u8; 32], (oneshot::Sender<ServerMessage>, [u8; 32])>,
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    let level = if std::env::var("SECHAT_DEBUG").is_ok() {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    tracing_subscriber::fmt().with_max_level(level).init();
    let mailbox = initialize_mailbox()?;
    let connections: DashMap<[u8; 32], mpsc::Sender<ServerToClient>> = DashMap::new();
    let presence: DashMap<[u8; 32], Vec<PresenceEntry>> = DashMap::new();
    let pendingp2ps: DashMap<[u8; 32], (oneshot::Sender<ServerMessage>, [u8; 32])> = DashMap::new();
    let state = Arc::new(AppState {
        presence,
        connections,
        mailbox,
        pendingp2ps,
    });
    tracing::debug!("Initialized AppState");

    {
        let s = state.clone();
        tokio::spawn(async move { presence_cleanup_loop(&s).await });
    }

    let stun_port: u16 = std::env::var("STUN_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3478);
    tokio::spawn(async move {
        if let Err(e) = stun::run_stun_server(stun_port).await {
            tracing::error!("STUN server failed: {:?}", e);
        }
    });

    let app = Router::new()
        .route("/ws", any(ws_handler))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .with_state(state);
    tracing::debug!("Created router");

    let addr: SocketAddr = "0.0.0.0:3000".parse().unwrap();

    if std::env::var("SECHAT_DEV_INSECURE").is_ok() {
        tracing::warn!("SECHAT_DEV_INSECURE set — serving PLAIN ws:// on {addr} (no TLS)");
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("failed to bind listener");
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    } else {
        let cert_path = PathBuf::from(
            std::env::var("TLS_CERT").expect("TLS_CERT must be set (path to PEM cert chain)"),
        );
        let key_path = PathBuf::from(
            std::env::var("TLS_KEY").expect("TLS_KEY must be set (path to PEM private key)"),
        );
        let tls = RustlsConfig::from_pem_file(cert_path, key_path)
            .await
            .expect("failed to load TLS cert/key");
        tracing::debug!("Starting the app on {addr} (wss)");
        axum_server::bind_rustls(addr, tls)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();
    }

    Ok(())
}
