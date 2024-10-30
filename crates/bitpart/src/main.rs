mod actions;
pub mod api;
mod conversation;
mod data;
pub mod db;
pub mod error;
mod event;
mod services;
pub mod utils;

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use clap::Parser;
use sea_orm::{ConnectionTrait, Database};
//use sea_orm_migration::prelude::*;
use clap_verbosity_flag::Verbosity;
use std::net::SocketAddr;
use tracing_log::AsTrace;

use api::ApiState;
use db::migration::migrate;
use error::BitpartError;

/// The Bitpart server
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
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

#[tokio::main]
async fn main() -> Result<(), BitpartError> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_max_level(args.verbose.log_level_filter().as_trace())
        .init();

    println!("Server is running!");

    let uri = format!("sqlite://{}?mode=rwc", args.database);
    let db = Database::connect(&uri).await?;
    let key_query = format!("PRAGMA key = '{}';", args.key);
    db.execute_unprepared(&key_query).await?;
    migrate(&uri).await?;

    let state = ApiState {
        db,
        auth: args.auth,
    };

    let app = Router::new()
        .route("/api/v1/bots", post(api::post_bot))
        .route(
            "/api/v1/bots/:id",
            get(api::get_bot).delete(api::delete_bot),
        )
        .route("/api/v1/bots/:id/versions", get(api::get_bot_versions))
        .route(
            "/api/v1/bots/:id/versions/:id",
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
            "/api/v1/memories/:id",
            get(api::get_memory).delete(api::delete_memory),
        )
        .route("/api/v1/messages", get(api::get_messages))
        .route("/api/v1/state", get(api::get_state))
        .route("/api/v1/requests", post(api::post_request))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);

    let addr: SocketAddr = args.bind.parse().expect("Unable to parse bind address");

    axum_server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}
