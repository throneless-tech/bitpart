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
    is_pni: bool,
    address: &str,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let address = address.to_owned();
    let is_pni = if is_pni { 1 } else { 0 };
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        c.query_row(
            "SELECT identity_key FROM signal_identities WHERE channel_id = ?1 AND is_pni = ?2 AND address = ?3",
            params![channel_id, is_pni, address],
            |row| row.get::<_, Vec<u8>>(0)
        )
        .optional()
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn set(
    channel_id: &str,
    is_pni: bool,
    address: &str,
    identity_key: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let address = address.to_owned();
    let identity_key = identity_key.to_vec();
    let is_pni = if is_pni { 1 } else { 0 };
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_identities (channel_id, is_pni, address, identity_key) VALUES (?1, ?2, ?3, ?4) 
             ON CONFLICT(channel_id, is_pni, address) DO UPDATE SET identity_key = excluded.identity_key",
            params![channel_id, is_pni, address, identity_key]
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
                "CREATE TABLE signal_identities (
                    channel_id varchar NOT NULL,
                    is_pni integer NOT NULL,
                    address varchar NOT NULL,
                    identity_key blob NOT NULL,
                    PRIMARY KEY (channel_id, is_pni, address)
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
    async fn test_aci_pni_separation() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let address = "same_address";

        set(channel_id, false, address, b"aci_key", &pool)
            .await
            .unwrap();
        set(channel_id, true, address, b"pni_key", &pool)
            .await
            .unwrap();

        let aci_key = get(channel_id, false, address, &pool).await.unwrap();
        let pni_key = get(channel_id, true, address, &pool).await.unwrap();

        assert_eq!(aci_key, Some(b"aci_key".to_vec()));
        assert_eq!(pni_key, Some(b"pni_key".to_vec()));
    }

    #[tokio::test]
    async fn test_upsert_behavior() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let address = "test_address";

        set(channel_id, false, address, b"key1", &pool)
            .await
            .unwrap();
        set(channel_id, false, address, b"key2", &pool)
            .await
            .unwrap();

        let retrieved = get(channel_id, false, address, &pool).await.unwrap();
        assert_eq!(retrieved, Some(b"key2".to_vec()));
    }
}
