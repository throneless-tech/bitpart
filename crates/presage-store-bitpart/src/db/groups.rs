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

pub async fn get_group(
    channel_id: &str,
    master_key: &[u8],
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let master_key = master_key.to_vec();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        c.query_row(
            "SELECT group_data FROM signal_groups WHERE channel_id = ?1 AND master_key = ?2",
            params![channel_id, master_key],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn set_group(
    channel_id: &str,
    master_key: &[u8],
    group_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let master_key = master_key.to_vec();
    let group_data = group_data.to_vec();
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_groups (channel_id, master_key, group_data) VALUES (?1, ?2, ?3) 
             ON CONFLICT(channel_id, master_key) DO UPDATE SET group_data = excluded.group_data",
            params![channel_id, master_key, group_data],
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_all_groups(
    channel_id: &str,
    pool: &Pool,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut stmt =
            c.prepare("SELECT master_key, group_data FROM signal_groups WHERE channel_id = ?1")?;
        let rows = stmt
            .query_map(params![channel_id], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn remove_all_groups(channel_id: &str, pool: &Pool) -> Result<u64, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<u64> {
        let n = c.execute(
            "DELETE FROM signal_groups WHERE channel_id = ?1",
            params![channel_id],
        )?;
        Ok(n as u64)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_group_avatar(
    channel_id: &str,
    master_key: &[u8],
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let master_key = master_key.to_vec();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        c.query_row(
            "SELECT avatar_data FROM signal_group_avatars WHERE channel_id = ?1 AND master_key = ?2",
            params![channel_id, master_key],
            |row| row.get::<_, Vec<u8>>(0)
        )
        .optional()
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn set_group_avatar(
    channel_id: &str,
    master_key: &[u8],
    avatar_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let master_key = master_key.to_vec();
    let avatar_data = avatar_data.to_vec();
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_group_avatars (channel_id, master_key, avatar_data) VALUES (?1, ?2, ?3) 
             ON CONFLICT(channel_id, master_key) DO UPDATE SET avatar_data = excluded.avatar_data",
            params![channel_id, master_key, avatar_data]
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpool_sqlite::{Config, Runtime};

    async fn setup_test_pool() -> Pool {
        let config = Config::new(":memory:");
        let pool = config.create_pool(Runtime::Tokio1).unwrap();

        let conn = pool.get().await.unwrap();
        conn.interact(|c| {
            c.execute(
                "CREATE TABLE signal_groups (
                    channel_id varchar NOT NULL,
                    master_key blob NOT NULL,
                    group_data blob NOT NULL,
                    PRIMARY KEY (channel_id, master_key)
                )",
                [],
            )?;
            c.execute(
                "CREATE TABLE signal_group_avatars (
                    channel_id varchar NOT NULL,
                    master_key blob NOT NULL,
                    avatar_data blob NOT NULL,
                    PRIMARY KEY (channel_id, master_key)
                )",
                [],
            )?;
            Ok::<(), rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_get_all_groups() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";

        set_group(
            channel_id,
            b"key1_32_bytes_long_padding_here",
            b"data1",
            &pool,
        )
        .await
        .unwrap();
        set_group(
            channel_id,
            b"key2_32_bytes_long_padding_here",
            b"data2",
            &pool,
        )
        .await
        .unwrap();

        let all_groups = get_all_groups(channel_id, &pool).await.unwrap();
        assert_eq!(all_groups.len(), 2);
        assert!(all_groups.contains(&(
            b"key1_32_bytes_long_padding_here".to_vec(),
            b"data1".to_vec()
        )));
        assert!(all_groups.contains(&(
            b"key2_32_bytes_long_padding_here".to_vec(),
            b"data2".to_vec()
        )));
    }

    #[tokio::test]
    async fn test_upsert_behavior() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let master_key = b"test_master_key_32_bytes_long_ok";

        set_group(channel_id, master_key, b"data1", &pool)
            .await
            .unwrap();
        set_group(channel_id, master_key, b"data2", &pool)
            .await
            .unwrap();

        let retrieved = get_group(channel_id, master_key, &pool).await.unwrap();
        assert_eq!(retrieved, Some(b"data2".to_vec()));
    }
}
