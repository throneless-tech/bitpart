pub mod actions;
pub mod api;
pub mod conversation;
pub mod data;
pub mod db;
pub mod event;

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use clap::Args;
use clap_verbosity_flag::Verbosity;
use sea_orm::{ConnectionTrait, Database};
use std::net::SocketAddr;
use tracing_log::AsTrace;

use crate::error::BitpartError;

#[derive(Debug, Args)]
pub struct ServerArgs {
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
    State(state): State<api::ApiState>,
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

pub async fn init_server(server: ServerArgs) -> Result<(), BitpartError> {
    tracing_subscriber::fmt()
        .with_max_level(server.verbose.log_level_filter().as_trace())
        .init();

    println!("Server is running!");

    let uri = format!("sqlite://{}?mode=rwc", server.database);
    let db = Database::connect(&uri).await?;
    let key_query = format!("PRAGMA key = '{}';", server.key);
    db.execute_unprepared(&key_query).await?;
    db::migration::migrate(&uri).await?;
    let database = Database::connect(&uri).await?;

    let runners = db::runner::list(None, None, &db).await?;

    for id in runners.iter() {
        let db = database.clone();
        let id = id.clone();
        // tokio::spawn(async move {
        //     start_channel(&id, &db).await;
        // });
    }

    let state = api::ApiState {
        db: database,
        auth: server.auth,
    };

    let app = Router::new()
        .route("/api/v1/bots", post(api::post_bot))
        .route(
            "/api/v1/bots/:id",
            get(api::get_bot).delete(api::delete_bot),
        )
        .route("/api/v1/bots", get(api::list_bots))
        .route("/api/v1/bots/:id/versions", get(api::get_bot_versions))
        .route(
            "/api/v1/bots/:id/versions/:id",
            get(api::get_bot_version).delete(api::delete_bot_version),
        )
        .route(
            "/api/v1/runners",
            post(api::post_runner).get(api::get_runners),
        )
        .route(
            "/api/v1/channels/:id",
            get(api::get_runner).delete(api::delete_runner),
        )
        // .route("/api/v1/channels/:id/link", post(api::link_device_channel))
        // .route("/api/v1/channels/:id/add", post(api::add_device_channel))
        .route("/api/v1/requests", post(api::post_request))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);

    let addr: SocketAddr = server.bind.parse().expect("Unable to parse bind address");

    axum_server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}
