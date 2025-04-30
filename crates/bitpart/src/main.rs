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
use bitpart_common::error::{BitpartError, Result};
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use directories::ProjectDirs;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use figment_file_provider_adapter::FileAdapter;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::{metrics::SdkMeterProvider, trace::SdkTracer};
use sea_orm::{ConnectOptions, Database};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::sync::oneshot;
use tracing::info;
use tracing_log::AsTrace;
use tracing_opentelemetry::MetricsLayer;
use tracing_subscriber::prelude::*;

use api::ApiState;
use channels::signal;
use db::migration::migrate;

/// Bitpart is a messaging tool that runs on top of Signal to support activists, journalists, and human rights defenders.
#[derive(Debug, Parser, Serialize, Deserialize)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Verbosity
    #[command(flatten)]
    verbose: Verbosity,

    /// API authentication token
    #[arg(short, long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    auth: Option<String>,

    /// IP address and port to bind to
    #[arg(short, long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    bind: Option<String>,

    /// Path to sqlcipher database file
    #[arg(short, long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    database: Option<String>,

    /// Database encryption key
    #[arg(short, long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    key: Option<String>,

    /// Enable Opentelemetry
    #[arg(short, long)]
    opentelemetry: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    /// Verbosity
    verbose: Verbosity,

    /// API authentication token
    auth: String,

    /// IP address and port to bind to
    bind: String,

    /// Path to sqlcipher database file
    database: String,

    /// Database encryption key
    key: String,

    /// Enable Opentelemetry
    opentelemetry: bool,
}

async fn authenticate(
    State(state): State<ApiState>,
    req: Request,
    next: Next,
) -> std::result::Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    match auth_header {
        Some(auth_header) if auth_header == state.auth => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

fn telemetry_tracer_init() -> Result<SdkTracer> {
    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder().with_http();

    let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(otlp_exporter.build()?)
        .build();

    Ok(tracer_provider.tracer("bitpart_tracer"))
}

fn telemetry_meter_init() -> Result<SdkMeterProvider> {
    let metric_exporter = opentelemetry_otlp::MetricExporter::builder().with_http();

    let meter_provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter.build()?)
        .build();

    Ok(meter_provider)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set project directories
    let proj_dirs = ProjectDirs::from("tech", "throneless", "bitpart").ok_or(
        BitpartError::Directory("Failed to find project directories.".to_owned()),
    )?;

    // Merge the configuration from CLI, environment, files, container secrets
    let server: Config = Figment::new()
        .merge(FileAdapter::wrap(Toml::file(
            proj_dirs.config_dir().join("config.toml"),
        )))
        .merge(FileAdapter::wrap(Env::prefixed("BITPART_")))
        .merge(Serialized::defaults(Cli::parse()))
        .extract()?;

    // Setup logging and telemetry
    if server.opentelemetry {
        tracing_subscriber::registry()
            .with(server.verbose.log_level_filter().as_trace())
            .with(tracing_subscriber::fmt::layer())
            .with(tracing_opentelemetry::layer().with_tracer(telemetry_tracer_init()?))
            .with(MetricsLayer::new(telemetry_meter_init()?))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(server.verbose.log_level_filter().as_trace())
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    // Initialize database
    let uri = format!("sqlite://{}?mode=rwc", server.database);
    let mut opts = ConnectOptions::new(&uri);
    opts.sqlcipher_key(server.key);
    let db = Database::connect(opts).await?;
    migrate(&db).await?;

    // Start incoming message channels
    let channels = db::channel::list(None, None, &db).await?;
    let state = ApiState {
        db,
        auth: server.auth,
        manager: Box::new(signal::SignalManager::new()),
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

    // Run client API
    let app = Router::new()
        .route("/ws", any(socket::handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);

    println!("Server is running 🤖");
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
        let _ = tokio::fs::remove_file(&path).await;
        let listener = tokio::net::UnixListener::bind(path).expect("Unable to bind to address");
        axum::serve(listener, app.into_make_service()).await?;
    };

    Ok(())
}
