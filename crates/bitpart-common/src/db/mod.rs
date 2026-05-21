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

use deadpool_sqlite::{Config, Hook, HookError, Runtime};
use std::path::Path;

use crate::error::{BitpartErrorKind, Result};

pub mod migration;

pub type Pool = deadpool_sqlite::Pool;

pub const DEFAULT_POOL_SIZE: usize = 32;

pub fn build_pool(path: &Path, key: String, size: usize) -> Result<Pool> {
    let cfg = Config::new(path);
    let key_for_hook = key.clone();
    let pool = cfg
        .builder(Runtime::Tokio1)
        .map_err(|e| BitpartErrorKind::Pool(format!("deadpool builder: {e}")))?
        .max_size(size)
        .post_create(Hook::async_fn(move |obj, _metrics| {
            let key = key_for_hook.clone();
            Box::pin(async move {
                obj.interact(move |conn| -> rusqlite::Result<()> {
                    conn.pragma_update(None, "key", &key)?;
                    conn.pragma_update(None, "busy_timeout", 5000)?;
                    Ok(())
                })
                .await
                .map_err(|e| HookError::message(format!("interact: {e}")))?
                .map_err(|e| HookError::message(format!("pragma: {e}")))?;
                Ok(())
            })
        }))
        .build()
        .map_err(|e| BitpartErrorKind::Pool(format!("deadpool build: {e}")))?;
    Ok(pool)
}
