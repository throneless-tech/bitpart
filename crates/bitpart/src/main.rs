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

pub mod api;
mod channels;
mod csml;
pub mod db;
pub mod error;
mod socket;
mod utils;

use axum::{
    Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::Response,
    routing::any,
};
use channels::signal;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use directories::ProjectDirs;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use figment_file_provider_adapter::FileAdapter;
use sea_orm::{ConnectOptions, Database};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::sync::oneshot;
use tracing::info;
use tracing_log::AsTrace;

use api::ApiState;
use db::migration::migrate;
use error::BitpartError;

/// The Bitpart server
#[derive(Debug, Parser, Serialize, Deserialize)]
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
    let proj_dirs = ProjectDirs::from("tech", "throneless", "bitpart").ok_or(
        BitpartError::Directory("Failed to find project directories.".to_owned()),
    )?;
    let server: Cli = Figment::new()
        .merge(Serialized::defaults(Cli::parse()))
        .merge(FileAdapter::wrap(Toml::file(
            proj_dirs.config_dir().join("config.toml"),
        )))
        .merge(FileAdapter::wrap(Env::prefixed("BITPART_")))
        .extract()?;
    tracing_subscriber::fmt()
        .with_max_level(server.verbose.log_level_filter().as_trace())
        .init();

    info!("Server is running!");

    let uri = format!("sqlite://{}?mode=rwc", server.database);
    let mut opts = ConnectOptions::new(&uri);
    opts.sqlcipher_key(server.key);
    let db = Database::connect(opts).await?;
    migrate(&db).await?;

    let channels = db::channel::list(None, None, &db).await?;
    let state = ApiState {
        db,
        auth: server.auth,
        manager: signal::SignalManager::new(),
    };
    for channel in channels.iter() {
        let (send, recv) = oneshot::channel();
        let contents = signal::ChannelMessageContents::StartChannel {
            id: channel.id.to_owned(),
            attachments_dir: proj_dirs.cache_dir().to_path_buf(),
        };
        let msg = signal::ChannelMessage {
            msg: contents,
            db: state.db.clone(),
            sender: send,
        };
        state.manager.send(msg);
        let res = recv.await?;
        info!("Started channel: {}", res);
    }

    let app = Router::new()
        .route("/ws", any(socket::handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);

    if let Ok(addr) = server.bind.parse::<SocketAddr>() {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("Unable to bind to address");
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;
    } else {
        let Ok(path) = server.bind.parse::<PathBuf>();
        let listener = tokio::net::UnixListener::bind(path).expect("Unable to bind to address");
        axum::serve(listener, app.into_make_service()).await?;
    };

    Ok(())
}
