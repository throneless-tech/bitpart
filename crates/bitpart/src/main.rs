pub mod api;
mod channels;
mod csml;
pub mod db;
pub mod error;
mod socket;
mod utils;

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::any,
    Router,
};
use channels::signal;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use sea_orm::{ConnectionTrait, Database};
use std::net::SocketAddr;
use tokio::sync::oneshot;
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
        .route("/ws", any(socket::handler))
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
