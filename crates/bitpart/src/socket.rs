// Bitpart
// Copyright (C) 2025 Throneless Tech

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{ConnectInfo, State},
    response::IntoResponse,
};
use bitpart_common::{
    error::{BitpartError, BitpartErrorKind, Result},
    socket::{Response, SocketMessage},
};
use serde::Serialize;
use std::net::SocketAddr;
use tracing::{debug, error};

use crate::api;
use crate::api::ApiState;

pub async fn handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state))
}

async fn handle_socket(mut socket: WebSocket, who: SocketAddr, mut state: ApiState) {
    while let Some(msg) = socket.recv().await {
        let msg = if let Ok(msg) = msg {
            match process_message(msg, who, &mut state).await {
                Ok(Some(msg)) => msg,
                Ok(None) => {
                    debug!("Websocket closed");
                    return;
                }
                Err(err) => {
                    error!("Error parsing message from {who}: {}", err);
                    return;
                }
            }
        } else {
            error!("Client {who} abruptly disconnected");
            return;
        };

        if socket.send(msg).await.is_err() {
            error!("Client {who} abruptly disconnected");
            return;
        }
    }
}

fn wrap_error<S: Serialize>(response_type: &str, res: &S) -> Result<Option<Message>> {
    Ok(Some(Message::Text(
        serde_json::to_string(&SocketMessage::Error(Response {
            response_type: response_type.to_owned(),
            response: res,
        }))?
        .into(),
    )))
}

fn wrap_response<S: Serialize>(response_type: &str, res: &S) -> Result<Option<Message>> {
    Ok(Some(Message::Text(
        serde_json::to_string(&SocketMessage::Response(Response {
            response_type: response_type.to_owned(),
            response: res,
        }))?
        .into(),
    )))
}

trait ApiResultExt {
    fn into_ws(self, response_type: &str) -> Result<Option<Message>>;
}

impl<T: Serialize> ApiResultExt for std::result::Result<T, BitpartError> {
    fn into_ws(self, response_type: &str) -> Result<Option<Message>> {
        match self {
            Ok(res) => wrap_response(response_type, &res),
            Err(err) => wrap_error(response_type, &err.to_string()),
        }
    }
}

async fn process_message(
    msg: Message,
    who: SocketAddr,
    state: &mut ApiState,
) -> Result<Option<Message>> {
    match msg {
        Message::Text(t) => {
            debug!(">>> {who} sent str: {t:?}");
            let contents: SocketMessage<String> = serde_json::from_slice(t.as_bytes())?;
            match contents {
                SocketMessage::CreateBot(bot) => {
                    api::create_bot(*bot, state).await.into_ws("CreateBot")
                }
                SocketMessage::ReadBot { id } => api::read_bot(&id, state).await.into_ws("ReadBot"),
                SocketMessage::BotVersions { id, options } => {
                    let (limit, offset) =
                        options.map(|p| (p.limit, p.offset)).unwrap_or((None, None));
                    api::get_bot_versions(&id, limit, offset, state)
                        .await
                        .into_ws("BotVersions")
                }
                SocketMessage::RollbackBot { id, version_id } => {
                    api::touch_bot_version(&id, &version_id, state)
                        .await
                        .into_ws("RollbackBot")
                }
                SocketMessage::DiffBot {
                    version_a,
                    version_b,
                } => api::get_bot_diff(&version_a, &version_b, state)
                    .await
                    .into_ws("DiffBot"),
                SocketMessage::DeleteBot { id } => {
                    api::delete_bot(&id, state).await.into_ws("DeleteBot")
                }
                SocketMessage::ListBots(options) => {
                    let (limit, offset) =
                        options.map(|p| (p.limit, p.offset)).unwrap_or((None, None));
                    api::list_bots(limit, offset, state)
                        .await
                        .into_ws("ListBots")
                }
                SocketMessage::CreateChannel { id, bot_id } => {
                    api::create_channel(&id, &bot_id, state)
                        .await
                        .into_ws("CreateChannel")
                }
                SocketMessage::ReadChannel { id, bot_id } => api::read_channel(&id, &bot_id, state)
                    .await
                    .into_ws("ReadChannel"),
                SocketMessage::ResetChannel { id, bot_id } => {
                    api::reset_channel(&id, &bot_id, state)
                        .await
                        .into_ws("ResetChannel")
                }
                SocketMessage::ListChannels(options) => {
                    let (limit, offset) =
                        options.map(|p| (p.limit, p.offset)).unwrap_or((None, None));
                    api::list_channels(limit, offset, state)
                        .await
                        .into_ws("ListChannels")
                }
                SocketMessage::DeleteChannel { id, bot_id } => {
                    api::delete_channel(&id, &bot_id, state)
                        .await
                        .into_ws("DeleteChannel")
                }
                SocketMessage::ChatRequest(req) => api::process_request(&req, &state.pool)
                    .await
                    .into_ws("ChatRequest"),
                SocketMessage::LinkChannel {
                    id,
                    bot_id,
                    device_name,
                } => api::link_channel(
                    &id,
                    &bot_id,
                    &device_name,
                    state.attachments_dir.clone(),
                    state,
                )
                .await
                .into_ws("LinkChannel"),
                _ => Ok(wrap_error(
                    "SocketMessage",
                    &"Invalid SocketMessage".to_owned(),
                )?),
            }
        }
        Message::Binary(d) => {
            debug!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
            Ok(wrap_error(
                "BinaryFrame",
                &"Server doesn't accept binary frames".to_owned(),
            )?)
        }
        Message::Close(c) => {
            if let Some(cf) = c {
                debug!(
                    ">>> {} sent close with code {} and reason `{}`",
                    who, cf.code, cf.reason
                );
                match cf.code {
                    1000 => Ok(None), // 1000 is code for "Normal"
                    _ => Err(BitpartErrorKind::WebsocketClose.into()),
                }
            } else {
                debug!(">>> {who} somehow sent close message without CloseFrame");
                Err(BitpartErrorKind::WebsocketClose.into())
            }
        }

        Message::Pong(v) => {
            debug!(">>> {who} sent pong with {v:?}");
            Ok(Some(Message::Text(
                serde_json::to_string("Pong received")?.into(),
            )))
        }
        Message::Ping(v) => {
            debug!(">>> {who} sent ping with {v:?}");
            Ok(Some(Message::Text(
                serde_json::to_string("Ping received")?.into(),
            )))
        }
    }
}
