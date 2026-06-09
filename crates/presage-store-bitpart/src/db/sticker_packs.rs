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

pub async fn get(
    channel_id: &str,
    pack_id: &[u8],
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let pack_id = pack_id.to_vec();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        c.query_row(
            "SELECT pack_data FROM signal_sticker_packs WHERE channel_id = ?1 AND pack_id = ?2",
            params![channel_id, pack_id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn set(
    channel_id: &str,
    pack_id: &[u8],
    pack_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let pack_id = pack_id.to_vec();
    let pack_data = pack_data.to_vec();
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_sticker_packs (channel_id, pack_id, pack_data) VALUES (?1, ?2, ?3) 
             ON CONFLICT(channel_id, pack_id) DO UPDATE SET pack_data = excluded.pack_data",
            params![channel_id, pack_id, pack_data],
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_all(
    channel_id: &str,
    pool: &Pool,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut stmt =
            c.prepare("SELECT pack_id, pack_data FROM signal_sticker_packs WHERE channel_id = ?1")?;
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

pub async fn remove(
    channel_id: &str,
    pack_id: &[u8],
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let pack_id = pack_id.to_vec();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        let pack_data: Option<Vec<u8>> = c
            .query_row(
                "SELECT pack_data FROM signal_sticker_packs WHERE channel_id = ?1 AND pack_id = ?2",
                params![channel_id, pack_id],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?;

        if pack_data.is_some() {
            c.execute(
                "DELETE FROM signal_sticker_packs WHERE channel_id = ?1 AND pack_id = ?2",
                params![channel_id, pack_id],
            )?;
        }

        Ok(pack_data)
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
                "CREATE TABLE signal_sticker_packs (
                    channel_id varchar NOT NULL,
                    pack_id blob NOT NULL,
                    pack_data blob NOT NULL,
                    PRIMARY KEY (channel_id, pack_id)
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
    async fn test_set_get_remove() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let pack_id = b"test_pack_id_16b";
        let pack_data = b"test_sticker_pack_data";

        assert_eq!(get(channel_id, pack_id, &pool).await.unwrap(), None);

        set(channel_id, pack_id, pack_data, &pool).await.unwrap();

        let retrieved = get(channel_id, pack_id, &pool).await.unwrap();
        assert_eq!(retrieved, Some(pack_data.to_vec()));

        let removed = remove(channel_id, pack_id, &pool).await.unwrap();
        assert_eq!(removed, Some(pack_data.to_vec()));

        assert_eq!(get(channel_id, pack_id, &pool).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_get_all() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";

        set(channel_id, b"pack1_id_16bytes", b"pack1_data", &pool)
            .await
            .unwrap();
        set(channel_id, b"pack2_id_16bytes", b"pack2_data", &pool)
            .await
            .unwrap();

        let all_packs = get_all(channel_id, &pool).await.unwrap();
        assert_eq!(all_packs.len(), 2);
        assert!(all_packs.contains(&(b"pack1_id_16bytes".to_vec(), b"pack1_data".to_vec())));
        assert!(all_packs.contains(&(b"pack2_id_16bytes".to_vec(), b"pack2_data".to_vec())));
    }

    #[tokio::test]
    async fn test_upsert_behavior() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let pack_id = b"test_pack_id_16b";

        set(channel_id, pack_id, b"data1", &pool).await.unwrap();
        set(channel_id, pack_id, b"data2", &pool).await.unwrap();

        let retrieved = get(channel_id, pack_id, &pool).await.unwrap();
        assert_eq!(retrieved, Some(b"data2".to_vec()));
    }
}
