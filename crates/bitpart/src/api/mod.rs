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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bitpart_common::db::Pool;
use tokio::sync::Mutex;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::channels::signal;

pub mod bot;
pub mod channel;
pub mod request;

pub use bot::{
    create_bot, delete_bot, delete_bot_version, get_bot_diff, get_bot_version, get_bot_versions,
    list_bots, read_bot, touch_bot_version,
};
pub use channel::{
    create_channel, delete_channel, link_channel, list_channels, read_channel, reset_channel,
    start_channel,
};
pub use request::process_request;

#[derive(Clone)]
pub struct ApiState {
    pub pool: Pool,
    pub auth: String,
    pub parent_token: CancellationToken,
    pub tokens: Arc<Mutex<HashMap<(String, String), CancellationToken>>>,
    pub tracker: TaskTracker,
    pub attachments_dir: PathBuf,
    pub manager: Arc<dyn signal::ChannelBackend>,
}
