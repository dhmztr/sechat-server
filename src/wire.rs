use crate::*;
use axum::{
    Error,
    extract::ws::{Message, WebSocket},
};
use futures::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use tokio::sync::mpsc;

pub async fn handle_receiving(msg: Result<Message, Error>) -> Result<ClientToServer, ServerError> {
    let data = msg.map_err(|_| ServerError::WebsocketReceiveFailed)?;
    let ds = data.into_data().to_vec();
    rmp_serde::from_slice::<ClientToServer>(&ds)
        .map_err(|e| ServerError::DeserializationFailed(e.to_string()))
}

pub async fn parse_and_send(
    msg: &ServerToClient,
    socket: &mut WebSocket,
) -> Result<(), ServerError> {
    let item = rmp_serde::to_vec(msg).map_err(|_| ServerError::SerializationFailed)?;
    socket
        .send(Message::from(item))
        .await
        .map_err(|_| ServerError::WebsocketSendFailed)?;
    Ok(())
}

pub fn parse_message_from_client(
    msg: Option<Result<Message, Error>>,
) -> Result<ClientToServer, ServerError> {
    if let Some(data) = msg {
        let msgbytes = data.map_err(|_| ServerError::WebsocketReceiveFailed)?;
        let bytes = msgbytes.into_data();
        let parsed = rmp_serde::from_slice::<ClientToServer>(bytes.iter().as_slice())
            .map_err(|e| ServerError::DeserializationFailed(e.to_string()))?;

        return Ok(parsed);
    } else {
        return Err(ServerError::WebsocketReceiveFailed);
    }
}

pub fn prepare_message(msg: ServerToClient) -> Result<Message, ServerError> {
    let bytes = rmp_serde::to_vec(&msg).map_err(|_| ServerError::SerializationFailed)?;
    Ok(Message::from(bytes))
}

pub async fn write_loop(
    mut rx: mpsc::Receiver<ServerToClient>,
    mut writer: SplitSink<WebSocket, Message>,
) -> Result<(), ServerError> {
    while let Some(message) = rx.recv().await {
        let ready_to_send = prepare_message(message)?;
        writer
            .send(ready_to_send)
            .await
            .map_err(|_| ServerError::WebsocketSendFailed)?;
    }
    Ok(())
}

pub async fn read_loop(
    mut reader: SplitStream<WebSocket>,
    tx: mpsc::Sender<ClientToServer>,
) -> Result<(), ServerError> {
    while let Some(incoming) = reader.next().await {
        let parsed = parse_message_from_client(Some(incoming))?;
        tx.send(parsed)
            .await
            .map_err(|_| ServerError::MpscSendFailed)?;
    }
    Ok(())
}
