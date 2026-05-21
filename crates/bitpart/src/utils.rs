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
use crate::channels::signal::{ChannelBackend, ChannelMessage};
#[cfg(test)]
use crate::{api::ApiState, socket};
#[cfg(test)]
use axum::{Router, routing::any};
#[cfg(test)]
use axum_test::{TestServer, TestWebSocket};
#[cfg(test)]
use bitpart_common::{
    db::{build_pool, migration::migrate},
    error::Result,
};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::net::SocketAddr;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::Mutex;
#[cfg(test)]
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[cfg(test)]
pub struct MockChannelBackend;

#[cfg(test)]
#[async_trait::async_trait]
impl ChannelBackend for MockChannelBackend {
    async fn send(&self, msg: ChannelMessage) -> Result<()> {
        let _ = msg.sender.send("".to_owned());
        Ok(())
    }
}

#[cfg(test)]
pub async fn get_test_socket() -> TestWebSocket {
    // File-backed: deadpool's `:memory:` gives each connection its own
    // private DB.
    let dir = Box::leak(Box::new(tempfile::tempdir().expect("tempdir")));
    let path = dir.path().join("bitpart-test.sqlite");
    let key = "bitparttestkey";

    let pool = build_pool(&path, key.to_owned(), 4).expect("build pool");
    migrate(&pool).await.expect("rusqlite migrator");

    let token = CancellationToken::new();
    let tracker = TaskTracker::new();
    let tokens: HashMap<(String, String), CancellationToken> = HashMap::new();
    let state = ApiState {
        pool,
        parent_token: token.clone(),
        tokens: Arc::new(Mutex::new(tokens)),
        tracker: tracker.clone(),
        auth: "test".into(),
        attachments_dir: "/tmp".into(),
        manager: Arc::new(MockChannelBackend),
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
