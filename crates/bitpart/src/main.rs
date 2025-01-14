mod actions;
pub mod api;
mod channels;
mod conversation;
mod data;
pub mod db;
pub mod error;
mod event;
mod messages;
pub mod utils;

use axum::extract::connect_info::ConnectInfo;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Router,
};
use channels::signal;
use clap::Parser;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
//use sea_orm_migration::prelude::*;
use clap_verbosity_flag::Verbosity;
use presage::model::identity::OnNewIdentity;
use presage_store_bitpart::{BitpartStore, MigrationConflictStrategy};
use std::net::SocketAddr;
use tokio::sync::oneshot;
use tokio::task::LocalSet;
use tracing::{debug, error};
use tracing_log::AsTrace;

use api::ApiState;
use db::migration::migrate;
use error::BitpartError;
use messages::SocketMessage;

/// The Bitpart server
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Verbosity
    #[command(flatten)]
    verbose: Verbosity,

    /// API authentication token
    #[arg(short, long)]
    auth: String,

    /// IP address and port to bind to
    #[arg(short, long)]
    bind: String,

    /// Path to sqlcipher database file
    #[arg(short, long)]
    database: String,

    /// Database encryption key
    #[arg(short, long)]
    key: String,
}

async fn authenticate(
    State(state): State<ApiState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    match auth_header {
        Some(auth_header) if auth_header == state.auth => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTION
////////////////////////////////////////////////////////////////////////////////

async fn ws_handler(
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
                SocketMessage::CreateBot(bot) => {
                    let bot = api::create_bot(bot, state).await?;
                    Ok(Message::Text(serde_json::to_string(&bot)?.into()))
                }
                SocketMessage::ReadBot(id) => {
                    let bot = api::read_bot(id, state).await?;
                    Ok(Message::Text(serde_json::to_string(&bot)?.into()))
                }
                SocketMessage::DeleteBot(id) => {
                    let bot = api::delete_bot(id, state).await?;
                    Ok(Message::Text(serde_json::to_string(&bot)?.into()))
                }
                SocketMessage::ListBots => {
                    let list = api::list_bots(state).await?;
                    Ok(Message::Text(serde_json::to_string(&list)?.into()))
                }
                SocketMessage::CreateChannel(msg) => {
                    let channel = api::create_channel(&msg.id, &msg.bot_id, state).await?;
                    Ok(Message::Text(serde_json::to_string(&channel)?.into()))
                }
                SocketMessage::ReadChannel(id) => {
                    let channel = api::read_channel(&id, state).await?;
                    Ok(Message::Text(serde_json::to_string(&channel)?.into()))
                }
                SocketMessage::ListChannels(paginate) => {
                    let channels =
                        api::list_channels(paginate.limit, paginate.offset, state).await?;
                    Ok(Message::Text(serde_json::to_string(&channels)?.into()))
                }
                SocketMessage::DeleteChannel(id) => {
                    let bot = api::delete_channel(&id, state).await?;
                    Ok(Message::Text(serde_json::to_string(&bot)?.into()))
                }
                SocketMessage::ChatRequest(req) => {
                    let res = api::process_request(&req, state).await?;
                    Ok(Message::Text(serde_json::to_string(&res)?.into()))
                }
                SocketMessage::LinkChannel(link) => {
                    let (send, recv) = oneshot::channel();
                    let contents = signal::ChannelMessageContents::LinkChannel {
                        id: link.id,
                        device_name: link.device_name,
                    };
                    let msg = signal::ChannelMessage {
                        msg: contents,
                        db: state.db.clone(),
                        sender: send,
                    };
                    state.manager.send(msg);
                    let res = recv.await.unwrap();
                    Ok(Message::Text(res.into()))
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
                    let res = recv.await.unwrap();
                    Ok(Message::Text(res.into()))
                }
                _ => Ok(Message::Text(
                    serde_json::to_string("Invalid SocketMessage")?.into(),
                )),
            }
        }
        Message::Binary(d) => {
            println!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
            Ok(Message::Text(
                serde_json::to_string("Server doesn't accept binary frames")?.into(),
            ))
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

#[tokio::main]
async fn main() -> Result<(), BitpartError> {
    let server = Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(server.verbose.log_level_filter().as_trace())
        .init();

    println!("Server is running!");

    let uri = format!("sqlite://{}?mode=rwc", server.database);
    let db = Database::connect(&uri).await?;
    let key_query = format!("PRAGMA key = '{}';", server.key);
    db.execute_unprepared(&key_query).await?;
    migrate(&uri).await?;
    db.close().await?;

    let db = Database::connect(&uri).await?;
    let channels = db::channel::list(None, None, &db).await?;
    let state = ApiState {
        db,
        auth: server.auth,
        manager: signal::SignalManager::new(),
    };
    for id in channels.iter() {
        println!("Channel: {:?}", id);
        let (send, recv) = oneshot::channel();
        let contents = signal::ChannelMessageContents::StartChannel(id.to_owned());
        let msg = signal::ChannelMessage {
            msg: contents,
            db: state.db.clone(),
            sender: send,
        };
        state.manager.send(msg);
        let res = recv.await.unwrap();
        println!("Started channel: {}", res);
    }

    let app = Router::new()
        .route("/ws", any(ws_handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);

    let addr: SocketAddr = server.bind.parse().expect("Unable to parse bind address");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Unable to bind to address");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();

    Ok(())
}
