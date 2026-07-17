use crate::*;
use axum::{
    extract::{ConnectInfo, State, WebSocketUpgrade, ws::WebSocket},
    response::Response,
};
use chrono::Utc;
use dashmap::mapref::entry::Entry;
use futures::stream::StreamExt;
use std::sync::Arc;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    time::timeout,
};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, addr))
}

pub async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>, addr: SocketAddr) {
    let pub_key = match initial_handshake(&mut socket, &addr, &state).await {
        Ok(k) => k,
        Err(e) => {
            tracing::error!("Error happened in the handshake: {:#?}", e);
            return;
        }
    };
    let (sink, stream) = socket.split();

    let (out_tx, out_rx) = mpsc::channel::<ServerToClient>(100);
    let (in_tx, in_rx) = mpsc::channel::<ClientToServer>(100);
    if let Err(e) = insert_connection(pub_key, out_tx.clone(), &state) {
        tracing::error!("insert_client failed: {:?}", e);
    }
    let write_handle = tokio::spawn(write_loop(out_rx, sink));
    let read_handle = tokio::spawn(read_loop(stream, in_tx));
    let dispatch_handle = tokio::spawn(handle_message(out_tx, pub_key, state.clone(), in_rx));
    tokio::select! {
        _ = write_handle => {}
        _ = dispatch_handle=> {}
        _ = read_handle=> {}
    }
    cleanup_client(pub_key, &state).await
}

pub async fn initial_handshake(
    socket: &mut WebSocket,
    addr: &SocketAddr,
    state: &Arc<AppState>,
) -> Result<[u8; 32], ServerError> {
    let (ip, port) = (addr.ip(), addr.port());
    let address = format!("{ip}:{port}");
    let pubkey: [u8; 32];
    if let Some(msg) = socket.recv().await {
        let message = handle_receiving(msg).await?;
        let msg_timestamp = message.timestamp;
        match message.payload {
            ClientMessage::Auth {
                pub_key,
                verif,
                signature,
            } => {
                if verify_message(&pub_key, &verif, signature, msg_timestamp).is_ok() {
                    let id = identity_hash(&pub_key, &verif);
                    tracing::info!(identity = %hex::encode(id), %address, "client authenticated");
                    let authresponse = ServerMessage::AuthOk {
                        observed_address: address,
                    };
                    pubkey = id;
                    let response = ServerToClient::new(authresponse);
                    parse_and_send(&response, socket).await?;
                    let blobs = retrieve_blobs_for_peer(&state.mailbox.clone(), &id)?;
                    tracing::debug!(
                        identity = %hex::encode(id),
                        pending_blobs = blobs.len(),
                        "delivering pending blobs on connect"
                    );
                    send_blobs(&blobs, socket, &state, id).await?;
                } else {
                    tracing::warn!(
                        x25519 = %hex::encode(pub_key),
                        verifying = %hex::encode(verif),
                        "auth signature verification FAILED"
                    );
                    return Err(ServerError::InvalidSignature);
                }
            }
            _ => return Err(ServerError::HandshakeFailed),
        }
    } else {
        return Err(ServerError::WebsocketReceiveFailed);
    }
    Ok(pubkey)
}

pub async fn handle_message(
    socket: mpsc::Sender<ServerToClient>,
    pub_key: [u8; 32],
    state: Arc<AppState>,
    mut receiver: mpsc::Receiver<ClientToServer>,
) -> Result<(), ServerError> {
    let caller_hash: [u8; 32] = pub_key;
    while let Some(message) = receiver.recv().await {
        if (Utc::now().timestamp() - message.timestamp).abs() > 30 {
            return Err(ServerError::TimestampOutOfRange);
        }
        match message.payload {
            ClientMessage::Auth { .. } => {
                let response = ServerToClient::new(ServerMessage::Error {
                    reason: "Already verified".to_string(),
                });
                socket
                    .send(response)
                    .await
                    .map_err(|_| ServerError::MpscSendFailed)?;
            }

            ClientMessage::Announce { token, ip_port } => {
                let peers =
                    handle_presence(token, ip_port.clone(), message.timestamp, &pub_key, &state)
                        .await
                        .map_err(|_| ServerError::PresenceFailed)?;
                for (peer_pubkey, peer_ip) in peers {
                    let peer_hash: [u8; 32] = peer_pubkey;
                    if let Some(peertx) = retrieve_connection(peer_pubkey, &state) {
                        let my_msg = ServerMessage::PeerOnline {
                            hash: caller_hash,
                            ip_port: ip_port.clone(),
                        };
                        let _ = peertx.send(ServerToClient::new(my_msg)).await;
                    }

                    let peermessage = ServerToClient::new(ServerMessage::PeerOnline {
                        hash: peer_hash,
                        ip_port: peer_ip,
                    });
                    socket
                        .send(peermessage)
                        .await
                        .map_err(|_| ServerError::MpscSendFailed)?;
                }
            }
            ClientMessage::Unannounce { token } => {
                let peers = retrieve_peers(&state, token, &pub_key)?;
                let mut now_empty = false;
                if let Some(mut entries) = state.presence.get_mut(&token) {
                    entries.retain(|e| e.pub_key != pub_key);
                    now_empty = entries.is_empty();
                }
                if now_empty {
                    state.presence.remove(&token);
                }
                for (peer_pubkey, _peer_ip) in peers {
                    if let Some(conn) = retrieve_connection(peer_pubkey, &state) {
                        let msg = ServerMessage::PeerOffline { hash: caller_hash };
                        let _ = conn.send(ServerToClient::new(msg)).await;
                    }
                }
            }
            ClientMessage::SendBlob {
                recipient_hash,
                blob,
            } => store_blob(&state, recipient_hash, blob).await?,

            ClientMessage::AckBlob { blob_id } => delete_blob(&state, blob_id, caller_hash)?,
            ClientMessage::LookupPeer { token: _ } => (),

            ClientMessage::RequestHolePunch { token } => {
                let hp_result: Result<(), ServerError> = async {
                    let peer_pub_key = {
                        let entries = state.presence.get(&token).ok_or_else(|| {
                            ServerError::StorageFailed("no presence for token".to_owned())
                        })?;
                        entries
                            .iter()
                            .find(|e| e.pub_key != pub_key)
                            .map(|e| e.pub_key)
                            .ok_or_else(|| {
                                ServerError::StorageFailed("no peer in token".to_owned())
                            })?
                    };

                    let peer_tx = retrieve_connection(peer_pub_key, &state).ok_or_else(|| {
                        ServerError::StorageFailed("peer not connected".to_owned())
                    })?;

                    let (oneshot_tx, oneshot_rx) = oneshot::channel::<ServerMessage>();
                    match state.pendingp2ps.entry(token) {
                        Entry::Occupied(_) => {
                            return Ok(());
                        }
                        Entry::Vacant(v) => {
                            v.insert((oneshot_tx, pub_key));
                        }
                    }

                    let request = ServerMessage::RequestHolePunch { pub_key, token };
                    if peer_tx.send(ServerToClient::new(request)).await.is_err() {
                        state.pendingp2ps.remove(&token);
                        return Err(ServerError::MpscSendFailed);
                    }

                    let state_for_task = state.clone();
                    let out_tx_for_task = socket.clone();
                    tokio::spawn(async move {
                        let response = match timeout(Duration::from_secs(10), oneshot_rx).await {
                            Ok(Ok(msg)) => msg,
                            Ok(Err(_)) | Err(_) => {
                                state_for_task.pendingp2ps.remove(&token);
                                ServerMessage::RequestDenied {
                                    token,
                                    pub_key,
                                    reason: RequestDeniedReason::Timeout,
                                }
                            }
                        };
                        let _ = out_tx_for_task.send(ServerToClient::new(response)).await;
                    });
                    Ok(())
                }
                .await;
                if let Err(e) = hp_result {
                    tracing::warn!(error = ?e, "hole-punch request failed — notifying caller");
                    let _ = socket
                        .send(ServerToClient::new(ServerMessage::RequestDenied {
                            token,
                            pub_key: [0u8; 32],
                            reason: RequestDeniedReason::Timeout,
                        }))
                        .await;
                }
            }

            ClientMessage::RequestDenied { token } => {
                let dn_result: Result<(), ServerError> = async {
                    {
                        let pending = state.pendingp2ps.get(&token).ok_or_else(|| {
                            ServerError::StorageFailed("no pending request for token".to_owned())
                        })?;
                        if pending.value().1 == pub_key {
                            return Err(ServerError::StorageFailed(
                                "initiator cannot respond to own request".to_owned(),
                            ));
                        }
                    }

                    let (_, (oneshot_tx, _initiator)) = state
                        .pendingp2ps
                        .remove(&token)
                        .ok_or(ServerError::HandshakeFailed)?;

                    let _ = oneshot_tx.send(ServerMessage::RequestDenied {
                        token,
                        pub_key,
                        reason: RequestDeniedReason::PeerDeclined,
                    });
                    Ok(())
                }
                .await;
                if let Err(e) = dn_result {
                    tracing::warn!(error = ?e, "hole-punch deny failed (non-fatal)");
                }
            }

            ClientMessage::RequestAccepted { token } => {
                let ac_result: Result<(), ServerError> = async {
                    {
                        let pending = state.pendingp2ps.get(&token).ok_or_else(|| {
                            ServerError::StorageFailed("no pending request for token".to_owned())
                        })?;
                        if pending.value().1 == pub_key {
                            return Err(ServerError::StorageFailed(
                                "initiator cannot respond to own request".to_owned(),
                            ));
                        }
                    }

                    let (initiator_ip, initiator_hash, my_ip) = {
                        let entries = state.presence.get(&token).ok_or_else(|| {
                            ServerError::StorageFailed("no presence for token".to_owned())
                        })?;

                        let initiator =
                            entries
                                .iter()
                                .find(|e| e.pub_key != pub_key)
                                .ok_or_else(|| {
                                    ServerError::StorageFailed(
                                        "initiator not in presence".to_owned(),
                                    )
                                })?;
                        let me =
                            entries
                                .iter()
                                .find(|e| e.pub_key == pub_key)
                                .ok_or_else(|| {
                                    ServerError::StorageFailed("self not in presence".to_owned())
                                })?;

                        (
                            initiator.ip_port.clone(),
                            initiator.pub_key,
                            me.ip_port.clone(),
                        )
                    };

                    let punchtimestamp = Utc::now().timestamp() + 5;

                    // Each side learns the OTHER peer's identity so it can load keys
                    // by hash (robust) instead of matching a cached address.
                    let initiator_msg = ServerMessage::PunchHole {
                        token,
                        peer_hash: pub_key,
                        ip_port: my_ip,
                        punchtimestamp,
                    };

                    let my_msg = ServerMessage::PunchHole {
                        token,
                        peer_hash: initiator_hash,
                        ip_port: initiator_ip,
                        punchtimestamp,
                    };

                    let (_, (oneshot_tx, _initiator_pubkey)) =
                        state.pendingp2ps.remove(&token).ok_or_else(|| {
                            ServerError::StorageFailed(
                                "no pending request for this token".to_owned(),
                            )
                        })?;

                    let _ = oneshot_tx.send(initiator_msg);

                    socket
                        .send(ServerToClient::new(my_msg))
                        .await
                        .map_err(|_| ServerError::MpscSendFailed)?;
                    Ok(())
                }
                .await;
                if let Err(e) = ac_result {
                    tracing::warn!(error = ?e, "hole-punch accept failed (non-fatal)");
                }
            }
            ClientMessage::RelayData {
                recipient_hash,
                payload,
            } => {
                if let Some(conn) = retrieve_connection(recipient_hash, &state) {
                    let msg = ServerToClient::new(ServerMessage::RelayData {
                        sender_hash: caller_hash,
                        payload,
                    });
                    let _ = conn.send(msg).await;
                } else {
                    tracing::debug!(
                        recipient = %hex::encode(recipient_hash),
                        "relay dropped — recipient offline"
                    );
                }
            }
            _ => {}
        }
    }
    Ok(())
}
