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

#[cfg(test)]
use crate::channels::signal;
#[cfg(test)]
use crate::db;
#[cfg(test)]
use crate::{api::ApiState, socket};
#[cfg(test)]
use axum::{Router, routing::any};
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
        attachments_dir: "/tmp".into(),
        manager: Box::new(signal::SignalManager::new()),
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
