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
use rusqlite::{OptionalExtension, params};

use crate::error::BitpartStoreError;

fn pool_err(e: impl std::fmt::Display) -> BitpartStoreError {
    BitpartStoreError::Pool(e.to_string())
}

pub async fn get_profile(
    channel_id: &str,
    profile_hash: &str,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let profile_hash = profile_hash.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        c.query_row(
            "SELECT profile_data FROM signal_profiles WHERE channel_id = ?1 AND profile_hash = ?2",
            params![channel_id, profile_hash],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn set_profile(
    channel_id: &str,
    profile_hash: &str,
    profile_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let profile_hash = profile_hash.to_owned();
    let profile_data = profile_data.to_vec();
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_profiles (channel_id, profile_hash, profile_data) VALUES (?1, ?2, ?3) 
             ON CONFLICT(channel_id, profile_hash) DO UPDATE SET profile_data = excluded.profile_data",
            params![channel_id, profile_hash, profile_data]
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn remove_all_profiles(channel_id: &str, pool: &Pool) -> Result<u64, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<u64> {
        let n = c.execute(
            "DELETE FROM signal_profiles WHERE channel_id = ?1",
            params![channel_id],
        )?;
        Ok(n as u64)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_profile_key(
    channel_id: &str,
    uuid: &[u8],
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let uuid = uuid.to_vec();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        c.query_row(
            "SELECT profile_key FROM signal_profile_keys WHERE channel_id = ?1 AND uuid = ?2",
            params![channel_id, uuid],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn set_profile_key(
    channel_id: &str,
    uuid: &[u8],
    profile_key: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let uuid = uuid.to_vec();
    let profile_key = profile_key.to_vec();
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_profile_keys (channel_id, uuid, profile_key) VALUES (?1, ?2, ?3) 
             ON CONFLICT(channel_id, uuid) DO UPDATE SET profile_key = excluded.profile_key",
            params![channel_id, uuid, profile_key],
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_profile_avatar(
    channel_id: &str,
    profile_hash: &str,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let profile_hash = profile_hash.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        c.query_row(
            "SELECT avatar_data FROM signal_profile_avatars WHERE channel_id = ?1 AND profile_hash = ?2",
            params![channel_id, profile_hash],
            |row| row.get::<_, Vec<u8>>(0)
        )
        .optional()
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn set_profile_avatar(
    channel_id: &str,
    profile_hash: &str,
    avatar_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let profile_hash = profile_hash.to_owned();
    let avatar_data = avatar_data.to_vec();
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_profile_avatars (channel_id, profile_hash, avatar_data) VALUES (?1, ?2, ?3) 
             ON CONFLICT(channel_id, profile_hash) DO UPDATE SET avatar_data = excluded.avatar_data",
            params![channel_id, profile_hash, avatar_data]
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}
