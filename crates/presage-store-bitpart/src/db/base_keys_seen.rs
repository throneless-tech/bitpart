// presage-store-bitpart
// Copyright (C) 2025 Throneless Tech
//
// This code is derived in part from code from the Presage project:
// Copyright (C) 2024 Gabriel Féron

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use deadpool_sqlite::Pool;
use rusqlite::params;

use crate::error::BitpartStoreError;

fn pool_err(e: impl std::fmt::Display) -> BitpartStoreError {
    BitpartStoreError::Pool(e.to_string())
}

pub async fn set(
    channel_id: &str,
    is_pni: bool,
    kyber_pre_key_id: u32,
    signed_pre_key_id: u32,
    base_key: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let base_key = base_key.to_vec();
    let is_pni = if is_pni { 1 } else { 0 };
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_base_keys_seen (channel_id, is_pni, kyber_pre_key_id, signed_pre_key_id, base_key) 
             VALUES (?1, ?2, ?3, ?4, ?5) 
             ON CONFLICT(channel_id, is_pni, kyber_pre_key_id) 
             DO UPDATE SET signed_pre_key_id = excluded.signed_pre_key_id, base_key = excluded.base_key",
            params![channel_id, is_pni, kyber_pre_key_id, signed_pre_key_id, base_key]
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}
