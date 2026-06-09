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

async fn get_impl(
    table: &'static str,
    channel_id: &str,
    key_id: u32,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        let sql = format!(
            "SELECT record_data FROM {} WHERE channel_id = ?1 AND key_id = ?2",
            table
        );
        c.query_row(&sql, params![channel_id, key_id], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .optional()
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_aci(
    channel_id: &str,
    key_id: u32,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    get_impl("signal_pre_keys", channel_id, key_id, pool).await
}

pub async fn get_pni(
    channel_id: &str,
    key_id: u32,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    get_impl("signal_pni_pre_keys", channel_id, key_id, pool).await
}

async fn set_impl(
    table: &'static str,
    channel_id: &str,
    key_id: u32,
    record_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let record_data = record_data.to_vec();
    conn.interact(move |c| -> rusqlite::Result<()> {
        let sql = format!(
            "INSERT INTO {} (channel_id, key_id, record_data) VALUES (?1, ?2, ?3) 
             ON CONFLICT(channel_id, key_id) DO UPDATE SET record_data = excluded.record_data",
            table
        );
        c.execute(&sql, params![channel_id, key_id, record_data])?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn set_aci(
    channel_id: &str,
    key_id: u32,
    record_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    set_impl("signal_pre_keys", channel_id, key_id, record_data, pool).await
}

pub async fn set_pni(
    channel_id: &str,
    key_id: u32,
    record_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    set_impl("signal_pni_pre_keys", channel_id, key_id, record_data, pool).await
}

async fn remove_impl(
    table: &'static str,
    channel_id: &str,
    key_id: u32,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        let select_sql = format!(
            "SELECT record_data FROM {} WHERE channel_id = ?1 AND key_id = ?2",
            table
        );
        let record_data: Option<Vec<u8>> = c
            .query_row(&select_sql, params![channel_id, key_id], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .optional()?;

        if record_data.is_some() {
            let delete_sql = format!(
                "DELETE FROM {} WHERE channel_id = ?1 AND key_id = ?2",
                table
            );
            c.execute(&delete_sql, params![channel_id, key_id])?;
        }

        Ok(record_data)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn remove_aci(
    channel_id: &str,
    key_id: u32,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    remove_impl("signal_pre_keys", channel_id, key_id, pool).await
}

pub async fn remove_pni(
    channel_id: &str,
    key_id: u32,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    remove_impl("signal_pni_pre_keys", channel_id, key_id, pool).await
}

async fn remove_all_impl(
    table: &'static str,
    channel_id: &str,
    pool: &Pool,
) -> Result<u64, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<u64> {
        let sql = format!("DELETE FROM {} WHERE channel_id = ?1", table);
        let n = c.execute(&sql, params![channel_id])?;
        Ok(n as u64)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn remove_all_aci(channel_id: &str, pool: &Pool) -> Result<u64, BitpartStoreError> {
    remove_all_impl("signal_pre_keys", channel_id, pool).await
}

pub async fn remove_all_pni(channel_id: &str, pool: &Pool) -> Result<u64, BitpartStoreError> {
    remove_all_impl("signal_pni_pre_keys", channel_id, pool).await
}

async fn max_key_id_impl(
    table: &'static str,
    channel_id: &str,
    pool: &Pool,
) -> Result<Option<u32>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<u32>> {
        let sql = format!("SELECT MAX(key_id) FROM {} WHERE channel_id = ?1", table);
        c.query_row(&sql, params![channel_id], |row| {
            row.get::<_, Option<u32>>(0)
        })
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn max_key_id_aci(
    channel_id: &str,
    pool: &Pool,
) -> Result<Option<u32>, BitpartStoreError> {
    max_key_id_impl("signal_pre_keys", channel_id, pool).await
}

pub async fn max_key_id_pni(
    channel_id: &str,
    pool: &Pool,
) -> Result<Option<u32>, BitpartStoreError> {
    max_key_id_impl("signal_pni_pre_keys", channel_id, pool).await
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
                "CREATE TABLE signal_pre_keys (
                    channel_id varchar NOT NULL,
                    key_id integer NOT NULL,
                    record_data blob NOT NULL,
                    PRIMARY KEY (channel_id, key_id)
                )",
                [],
            )?;
            c.execute(
                "CREATE TABLE signal_pni_pre_keys (
                    channel_id varchar NOT NULL,
                    key_id integer NOT NULL,
                    record_data blob NOT NULL,
                    PRIMARY KEY (channel_id, key_id)
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
        let key_id = 42u32;
        let record_data = b"test_record_data";

        assert_eq!(get_aci(channel_id, key_id, &pool).await.unwrap(), None);

        set_aci(channel_id, key_id, record_data, &pool)
            .await
            .unwrap();

        let retrieved = get_aci(channel_id, key_id, &pool).await.unwrap();
        assert_eq!(retrieved, Some(record_data.to_vec()));

        let removed = remove_aci(channel_id, key_id, &pool).await.unwrap();
        assert_eq!(removed, Some(record_data.to_vec()));

        assert_eq!(get_aci(channel_id, key_id, &pool).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_max_key_id() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";

        assert_eq!(max_key_id_aci(channel_id, &pool).await.unwrap(), None);

        set_aci(channel_id, 5, b"data5", &pool).await.unwrap();
        set_aci(channel_id, 2, b"data2", &pool).await.unwrap();
        set_aci(channel_id, 10, b"data10", &pool).await.unwrap();

        let max_id = max_key_id_aci(channel_id, &pool).await.unwrap();
        assert_eq!(max_id, Some(10u32));
    }

    #[tokio::test]
    async fn test_upsert_behavior() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let key_id = 42u32;

        set_aci(channel_id, key_id, b"data1", &pool).await.unwrap();
        set_aci(channel_id, key_id, b"data2", &pool).await.unwrap();

        let retrieved = get_aci(channel_id, key_id, &pool).await.unwrap();
        assert_eq!(retrieved, Some(b"data2".to_vec()));
    }
}
