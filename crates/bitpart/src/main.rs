mod actions;
pub mod api;
mod channels;
mod conversation;
mod data;
pub mod db;
pub mod error;
mod event;
pub mod utils;

use axum::extract::connect_info::ConnectInfo;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Json, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Router,
};
use clap::Parser;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
//use sea_orm_migration::prelude::*;
use clap_verbosity_flag::Verbosity;
use presage::model::identity::OnNewIdentity;
use presage_store_bitpart::{BitpartStore, MigrationConflictStrategy};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::ops::ControlFlow;
use tokio::task::LocalSet;
use tracing::{debug, error};
use tracing_log::AsTrace;

use api::ApiState;
use db::migration::migrate;
use error::BitpartError;

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

async fn start_channel(id: &str, db: &DatabaseConnection) -> Result<(), BitpartError> {
    let store = BitpartStore::open(
        id,
        db,
        MigrationConflictStrategy::Raise,
        OnNewIdentity::Trust,
    )
    .await?;

    channels::signal::receive_from(store, true)
        .await
        .map_err(|e| BitpartError::Signal(e))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state))
}

async fn handle_socket(mut socket: WebSocket, who: SocketAddr, state: ApiState) {
    while let Some(msg) = socket.recv().await {
        let msg = if let Ok(msg) = msg {
            match process_message(msg, who, &state) {
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

#[derive(Serialize, Deserialize)]
enum SocketMessage {
    Register,
    Error(String),
}

fn process_message(
    msg: Message,
    who: SocketAddr,
    state: &ApiState,
) -> Result<Message, BitpartError> {
    match msg {
        Message::Text(t) => {
            println!(">>> {who} sent str: {t:?}");
            let contents: SocketMessage = serde_json::from_slice(t.as_bytes())?;
            match contents {
                SocketMessage::Register => {
                    debug!("Register message received!");
                }
                SocketMessage::Error(err) => {
                    error!(err);
                }
            }
        }
        Message::Binary(d) => {
            println!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
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
        }
        Message::Ping(v) => {
            println!(">>> {who} sent ping with {v:?}");
        }
    }
    Err(BitpartError::WebsocketClose)
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
    for id in channels.iter() {
        let id = id.clone();
        let handle = db.clone();
        std::thread::spawn(move || {
            let local = LocalSet::new();
            local.spawn_local(async move {
                let _ = start_channel(&id, &handle).await;
            });
        });
    }
    let state = ApiState {
        db,
        auth: server.auth,
    };

    let app = Router::new()
        .route("/api/v1/bots", post(api::post_bot))
        .route(
            "/api/v1/bots/{id}",
            get(api::get_bot).delete(api::delete_bot),
        )
        .route("/api/v1/bots", get(api::list_bots))
        .route("/api/v1/bots/{id}/versions", get(api::get_bot_versions))
        .route(
            "/api/v1/bots/{id}/versions/{id}",
            get(api::get_bot_version).delete(api::delete_bot_version),
        )
        .route(
            "/api/v1/conversations",
            // get(api::get_conversations).patch(api::patch_conversation),
            get(api::get_conversations),
        )
        .route(
            "/api/v1/memories",
            post(api::post_memory)
                .get(api::get_memories)
                .delete(api::delete_memories),
        )
        .route(
            "/api/v1/memories/{id}",
            get(api::get_memory).delete(api::delete_memory),
        )
        .route("/api/v1/messages", get(api::get_messages))
        .route("/api/v1/state", get(api::get_state))
        .route(
            "/api/v1/channels",
            post(api::post_channel).get(api::get_channels),
        )
        .route(
            "/api/v1/channels/{id}",
            get(api::get_channel).delete(api::delete_channel),
        )
        // .route("/api/v1/channels/:id/link", post(api::link_device_channel))
        // .route("/api/v1/channels/:id/add", post(api::add_device_channel))
        .route("/api/v1/requests", post(api::post_request))
        .route("/ws", any(ws_handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);

    let addr: SocketAddr = server.bind.parse().expect("Unable to parse bind address");

    axum_server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}
