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
    error::{BitpartError, Result},
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

async fn handle_socket(mut socket: WebSocket, who: SocketAddr, state: ApiState) {
    while let Some(msg) = socket.recv().await {
        let msg = if let Ok(msg) = msg {
            match process_message(msg, who, &state).await {
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

async fn process_message(
    msg: Message,
    who: SocketAddr,
    state: &ApiState,
) -> Result<Option<Message>> {
    match msg {
        Message::Text(t) => {
            debug!(">>> {who} sent str: {t:?}");
            let contents: SocketMessage<String> = serde_json::from_slice(t.as_bytes())?;
            match contents {
                SocketMessage::CreateBot(bot) => match api::create_bot(*bot, state).await {
                    Ok(res) => wrap_response("CreateBot", &res),
                    Err(err) => wrap_error("CreateBot", &err.to_string()),
                },
                SocketMessage::ReadBot { id } => match api::read_bot(&id, state).await {
                    Ok(res) => wrap_response("ReadBot", &res),
                    Err(err) => wrap_error("ReadBot", &err.to_string()),
                },
                SocketMessage::BotVersions { id, options } => {
                    if let Some(paginate) = options {
                        match api::get_bot_versions(&id, paginate.limit, paginate.offset, state)
                            .await
                        {
                            Ok(res) => wrap_response("BotVersions", &res),
                            Err(err) => wrap_error("BotVersions", &err.to_string()),
                        }
                    } else {
                        match api::get_bot_versions(&id, None, None, state).await {
                            Ok(res) => wrap_response("BotVersions", &res),
                            Err(err) => wrap_error("BotVersions", &err.to_string()),
                        }
                    }
                }
                SocketMessage::RollbackBot { id, version_id } => {
                    match api::touch_bot_version(&id, &version_id, state).await {
                        Ok(res) => wrap_response("RollbackBot", &res),
                        Err(err) => wrap_error("RollbackBot", &err.to_string()),
                    }
                }
                SocketMessage::DiffBot {
                    version_a,
                    version_b,
                } => match api::get_bot_diff(&version_a, &version_b, state).await {
                    Ok(res) => wrap_response("DiffBot", &res),
                    Err(err) => wrap_error("DiffBot", &err.to_string()),
                },
                SocketMessage::DeleteBot { id } => match api::delete_bot(&id, state).await {
                    Ok(res) => wrap_response("DeleteBot", &res),
                    Err(err) => wrap_error("DeleteBot", &err.to_string()),
                },
                SocketMessage::ListBots(options) => {
                    if let Some(paginate) = options {
                        match api::list_bots(paginate.limit, paginate.offset, state).await {
                            Ok(res) => wrap_response("ListBots", &res),
                            Err(err) => wrap_error("ListBots", &err.to_string()),
                        }
                    } else {
                        match api::list_bots(None, None, state).await {
                            Ok(res) => wrap_response("ListBots", &res),
                            Err(err) => wrap_error("ListBots", &err.to_string()),
                        }
                    }
                }
                SocketMessage::CreateChannel { id, bot_id } => {
                    match api::create_channel(&id, &bot_id, state).await {
                        Ok(res) => wrap_response("CreateChannel", &res),
                        Err(err) => wrap_error("CreateChannel", &err.to_string()),
                    }
                }
                SocketMessage::ReadChannel { id, bot_id } => {
                    match api::read_channel(&id, &bot_id, state).await {
                        Ok(res) => wrap_response("ReadChannel", &res),
                        Err(err) => wrap_error("ReadChannel", &err.to_string()),
                    }
                }
                SocketMessage::ListChannels(options) => {
                    if let Some(paginate) = options {
                        match api::list_channels(paginate.limit, paginate.offset, state).await {
                            Ok(res) => wrap_response("ListChannels", &res),
                            Err(err) => wrap_error("ListChannels", &err.to_string()),
                        }
                    } else {
                        match api::list_channels(None, None, state).await {
                            Ok(res) => wrap_response("ListChannels", &res),
                            Err(err) => wrap_error("ListChannels", &err.to_string()),
                        }
                    }
                }
                SocketMessage::DeleteChannel { id, bot_id } => {
                    match api::delete_channel(&id, &bot_id, state).await {
                        Ok(res) => wrap_response("DeleteChannel", &res),
                        Err(err) => wrap_error("DeleteChannel", &err.to_string()),
                    }
                }

                SocketMessage::ChatRequest(req) => {
                    match api::process_request(&req, &state.db).await {
                        Ok(res) => wrap_response("ChatRequest", &res),
                        Err(err) => wrap_error("ChatRequest", &err.to_string()),
                    }
                }
                SocketMessage::LinkChannel {
                    id,
                    bot_id,
                    device_name,
                } => match api::link_channel(&id, &bot_id, &device_name, state).await {
                    Ok(res) => wrap_response("LinkChannel", &res),
                    Err(err) => wrap_error("LinkChannel", &err.to_string()),
                },
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
                    _ => Err(BitpartError::WebsocketClose),
                }
            } else {
                debug!(">>> {who} somehow sent close message without CloseFrame");
                Err(BitpartError::WebsocketClose)
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
