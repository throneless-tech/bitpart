use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use clap::Parser;
use sea_orm::Database;
use std::env;
use std::net::SocketAddr;

use bitpart_server::{api, error::BitpartError};

/// The Bitpart interpreter
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// IP address and port to bind to
    #[arg(short, long)]
    bind: String,

    /// Path to sqlcipher database file
    #[arg(short, long)]
    database: String,
}

const API_BASE: &str = concat!("/api/v", env!("CARGO_PKG_VERSION_MAJOR"), "/");

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTION
////////////////////////////////////////////////////////////////////////////////

#[tokio::main]
async fn main() -> Result<(), BitpartError> {
    let args = Args::parse();

    println!("{}", args.bind);
    println!("{}", args.database);

    let db = Database::connect(format!("sqlite://{}?mode=rwc", args.database)).await?;

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
        .with_state(db);

    let addr: SocketAddr = args.bind.parse().expect("Unable to parse bind address");

    axum_server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
