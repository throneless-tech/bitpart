#[cfg(test)]
use crate::channels::signal;
#[cfg(test)]
use crate::db;
#[cfg(test)]
use crate::{api::ApiState, socket};
#[cfg(test)]
use axum::{routing::any, Router};
#[cfg(test)]
use axum_test::{TestServer, TestWebSocket};
#[cfg(test)]
use sea_orm::Database;
#[cfg(test)]
use sea_orm_migration::MigratorTrait;
#[cfg(test)]
use std::net::SocketAddr;

#[cfg(test)]
pub async fn get_test_socket() -> TestWebSocket {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db::migration::Migrator::refresh(&db).await.unwrap();

    let state = ApiState {
        db,
        auth: "test".into(),
        manager: signal::SignalManager::new(),
    };

    let app = Router::new()
        .route("/ws", any(socket::handler))
        .with_state(state);

    let server = TestServer::builder()
        .http_transport()
        .build(app.into_make_service_with_connect_info::<SocketAddr>())
        .unwrap();
    server.get_websocket("/ws").await.into_websocket().await
}
