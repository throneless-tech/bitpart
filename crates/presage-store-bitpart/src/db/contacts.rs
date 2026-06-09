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
    uuid: &[u8],
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let uuid = uuid.to_vec();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        c.query_row(
            "SELECT contact_data FROM signal_contacts WHERE channel_id = ?1 AND uuid = ?2",
            params![channel_id, uuid],
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
    uuid: &[u8],
    contact_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let uuid = uuid.to_vec();
    let contact_data = contact_data.to_vec();
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_contacts (channel_id, uuid, contact_data) VALUES (?1, ?2, ?3) 
             ON CONFLICT(channel_id, uuid) DO UPDATE SET contact_data = excluded.contact_data",
            params![channel_id, uuid, contact_data],
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
            c.prepare("SELECT uuid, contact_data FROM signal_contacts WHERE channel_id = ?1")?;
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

pub async fn remove_all(channel_id: &str, pool: &Pool) -> Result<u64, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<u64> {
        let n = c.execute(
            "DELETE FROM signal_contacts WHERE channel_id = ?1",
            params![channel_id],
        )?;
        Ok(n as u64)
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
                "CREATE TABLE signal_contacts (
                    channel_id varchar NOT NULL,
                    uuid blob NOT NULL,
                    contact_data blob NOT NULL,
                    PRIMARY KEY (channel_id, uuid)
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
    async fn test_get_all() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";

        set(channel_id, b"uuid1_16bytes123", b"contact1", &pool)
            .await
            .unwrap();
        set(channel_id, b"uuid2_16bytes123", b"contact2", &pool)
            .await
            .unwrap();

        let all_contacts = get_all(channel_id, &pool).await.unwrap();
        assert_eq!(all_contacts.len(), 2);
        assert!(all_contacts.contains(&(b"uuid1_16bytes123".to_vec(), b"contact1".to_vec())));
        assert!(all_contacts.contains(&(b"uuid2_16bytes123".to_vec(), b"contact2".to_vec())));
    }

    #[tokio::test]
    async fn test_upsert_behavior() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let uuid = b"test_uuid_16bytes";

        set(channel_id, uuid, b"contact1", &pool).await.unwrap();
        set(channel_id, uuid, b"contact2", &pool).await.unwrap();

        let retrieved = get(channel_id, uuid, &pool).await.unwrap();
        assert_eq!(retrieved, Some(b"contact2".to_vec()));
    }
}
