use crate::channels::signal;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{ConnectInfo, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::sync::oneshot;
use tracing::{debug, error};

use crate::api::ApiState;
use crate::error::BitpartError;
use csml_interpreter::data::CsmlBot;

use crate::api;
use crate::csml::data::Request;

#[derive(Serialize, Deserialize)]
struct PaginateMessage {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "message_type", content = "data")]
enum SocketMessage {
    CreateBot(CsmlBot),
    ReadBot(String),
    DeleteBot(String),
    ListBots,
    CreateChannel {
        id: String,
        bot_id: String,
    },
    ReadChannel(String),
    ListChannels(Option<PaginateMessage>),
    DeleteChannel(String),
    LinkChannel {
        id: String,
        device_name: String,
    },
    RegisterChannel {
        id: String,
        phone_number: String,
        captcha: String,
    },
    ChatRequest(Request),
    Response(String),
    Error(String),
}

pub async fn handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state))
}

async fn handle_socket(mut socket: WebSocket, who: SocketAddr, state: ApiState) {
    println!("handle_socket");
    while let Some(msg) = socket.recv().await {
        let msg = if let Ok(msg) = msg {
            match process_message(msg, who, &state).await {
                Ok(msg) => msg,
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

fn wrap_error<S: Serialize>(res: &S) -> Result<Message, BitpartError> {
    Ok(Message::Text(
        serde_json::to_string(&SocketMessage::Error(serde_json::to_string(res)?))?.into(),
    ))
}

fn wrap_response<S: Serialize>(res: &S) -> Result<Message, BitpartError> {
    Ok(Message::Text(
        serde_json::to_string(&SocketMessage::Response(serde_json::to_string(res)?))?.into(),
    ))
}

async fn process_message(
    msg: Message,
    who: SocketAddr,
    state: &ApiState,
) -> Result<Message, BitpartError> {
    println!("process_message");
    match msg {
        Message::Text(t) => {
            println!(">>> {who} sent str: {t:?}");
            let contents: SocketMessage = serde_json::from_slice(t.as_bytes())?;
            match contents {
                SocketMessage::CreateBot(bot) => match api::create_bot(bot, state).await {
                    Ok(res) => wrap_response(&res),
                    Err(err) => wrap_error(&err.to_string()),
                },
                SocketMessage::ReadBot(id) => match api::read_bot(id, state).await {
                    Ok(res) => wrap_response(&res),
                    Err(err) => wrap_error(&err.to_string()),
                },
                SocketMessage::DeleteBot(id) => match api::delete_bot(id, state).await {
                    Ok(res) => wrap_response(&res),
                    Err(err) => wrap_error(&err.to_string()),
                },
                SocketMessage::ListBots => match api::list_bots(state).await {
                    Ok(res) => wrap_response(&res),
                    Err(err) => wrap_error(&err.to_string()),
                },
                SocketMessage::CreateChannel { id, bot_id } => {
                    match api::create_channel(&id, &bot_id, state).await {
                        Ok(res) => wrap_response(&res),
                        Err(err) => wrap_error(&err.to_string()),
                    }
                }
                SocketMessage::ReadChannel(id) => match api::read_channel(&id, state).await {
                    Ok(res) => wrap_response(&res),
                    Err(err) => wrap_error(&err.to_string()),
                },
                SocketMessage::ListChannels(options) => {
                    if let Some(paginate) = options {
                        match api::list_channels(paginate.limit, paginate.offset, state).await {
                            Ok(res) => wrap_response(&res),
                            Err(err) => wrap_error(&err.to_string()),
                        }
                    } else {
                        match api::list_channels(None, None, state).await {
                            Ok(res) => wrap_response(&res),
                            Err(err) => wrap_error(&err.to_string()),
                        }
                    }
                }
                SocketMessage::DeleteChannel(id) => match api::delete_channel(&id, state).await {
                    Ok(res) => wrap_response(&res),
                    Err(err) => wrap_error(&err.to_string()),
                },

                SocketMessage::ChatRequest(req) => {
                    match api::process_request(&req, &state.db).await {
                        Ok(res) => wrap_response(&res),
                        Err(err) => wrap_error(&err.to_string()),
                    }
                }
                SocketMessage::LinkChannel { id, device_name } => {
                    let (send, recv) = oneshot::channel();
                    let contents = signal::ChannelMessageContents::LinkChannel { id, device_name };
                    let msg = signal::ChannelMessage {
                        msg: contents,
                        db: state.db.clone(),
                        sender: send,
                    };
                    state.manager.send(msg);
                    match recv.await {
                        Ok(res) => wrap_response(&res),
                        Err(err) => wrap_error(&err.to_string()),
                    }
                }
                SocketMessage::RegisterChannel {
                    id,
                    phone_number,
                    captcha,
                } => {
                    let (send, recv) = oneshot::channel();
                    let contents = signal::ChannelMessageContents::RegisterChannel {
                        id,
                        phone_number,
                        captcha,
                    };
                    let msg = signal::ChannelMessage {
                        msg: contents,
                        db: state.db.clone(),
                        sender: send,
                    };
                    state.manager.send(msg);
                    match recv.await {
                        Ok(res) => wrap_response(&res),
                        Err(err) => wrap_error(&err.to_string()),
                    }
                }
                _ => Ok(wrap_error(&"Invalid SocketMessage".to_owned())?),
            }
        }
        Message::Binary(d) => {
            println!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
            Ok(wrap_error(
                &"Server doesn't accept binary frames".to_owned(),
            )?)
        }
        Message::Close(c) => {
            if let Some(cf) = c {
                println!(
                    ">>> {} sent close with code {} and reason `{}`",
                    who, cf.code, cf.reason
                );
            } else {
                println!(">>> {who} somehow sent close message without CloseFrame");
            }
            return Err(BitpartError::WebsocketClose);
        }

        Message::Pong(v) => {
            println!(">>> {who} sent pong with {v:?}");
            Ok(Message::Text(
                serde_json::to_string("Pong received")?.into(),
            ))
        }
        Message::Ping(v) => {
            println!(">>> {who} sent ping with {v:?}");
            Ok(Message::Text(
                serde_json::to_string("Ping received")?.into(),
            ))
        }
    }
}
