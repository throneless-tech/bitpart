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
    get_impl("signal_kyber_pre_keys", channel_id, key_id, pool).await
}

pub async fn get_pni(
    channel_id: &str,
    key_id: u32,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    get_impl("signal_pni_kyber_pre_keys", channel_id, key_id, pool).await
}

async fn set_impl(
    table: &'static str,
    channel_id: &str,
    key_id: u32,
    record_data: &[u8],
    is_last_resort: bool,
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let record_data = record_data.to_vec();
    let is_last_resort = if is_last_resort { 1 } else { 0 };
    conn.interact(move |c| -> rusqlite::Result<()> {
        let sql = format!(
            "INSERT INTO {} (channel_id, key_id, record_data, is_last_resort) VALUES (?1, ?2, ?3, ?4) 
             ON CONFLICT(channel_id, key_id) DO UPDATE SET record_data = excluded.record_data, is_last_resort = excluded.is_last_resort",
            table
        );
        c.execute(&sql, params![channel_id, key_id, record_data, is_last_resort])?;
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
    is_last_resort: bool,
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    set_impl(
        "signal_kyber_pre_keys",
        channel_id,
        key_id,
        record_data,
        is_last_resort,
        pool,
    )
    .await
}

pub async fn set_pni(
    channel_id: &str,
    key_id: u32,
    record_data: &[u8],
    is_last_resort: bool,
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    set_impl(
        "signal_pni_kyber_pre_keys",
        channel_id,
        key_id,
        record_data,
        is_last_resort,
        pool,
    )
    .await
}

async fn get_all_impl(
    table: &'static str,
    channel_id: &str,
    pool: &Pool,
) -> Result<Vec<(u32, Vec<u8>, bool)>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Vec<(u32, Vec<u8>, bool)>> {
        let sql = format!(
            "SELECT key_id, record_data, is_last_resort FROM {} WHERE channel_id = ?1",
            table
        );
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt
            .query_map(params![channel_id], |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i32>(2)? != 0,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_all_aci(
    channel_id: &str,
    pool: &Pool,
) -> Result<Vec<(u32, Vec<u8>, bool)>, BitpartStoreError> {
    get_all_impl("signal_kyber_pre_keys", channel_id, pool).await
}

pub async fn get_all_pni(
    channel_id: &str,
    pool: &Pool,
) -> Result<Vec<(u32, Vec<u8>, bool)>, BitpartStoreError> {
    get_all_impl("signal_pni_kyber_pre_keys", channel_id, pool).await
}

async fn get_last_resort_impl(
    table: &'static str,
    channel_id: &str,
    pool: &Pool,
) -> Result<Vec<(u32, Vec<u8>)>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Vec<(u32, Vec<u8>)>> {
        let sql = format!(
            "SELECT key_id, record_data FROM {} WHERE channel_id = ?1 AND is_last_resort = 1",
            table
        );
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt
            .query_map(params![channel_id], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_last_resort_aci(
    channel_id: &str,
    pool: &Pool,
) -> Result<Vec<(u32, Vec<u8>)>, BitpartStoreError> {
    get_last_resort_impl("signal_kyber_pre_keys", channel_id, pool).await
}

pub async fn get_last_resort_pni(
    channel_id: &str,
    pool: &Pool,
) -> Result<Vec<(u32, Vec<u8>)>, BitpartStoreError> {
    get_last_resort_impl("signal_pni_kyber_pre_keys", channel_id, pool).await
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
    remove_impl("signal_kyber_pre_keys", channel_id, key_id, pool).await
}

pub async fn remove_pni(
    channel_id: &str,
    key_id: u32,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    remove_impl("signal_pni_kyber_pre_keys", channel_id, key_id, pool).await
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
    remove_all_impl("signal_kyber_pre_keys", channel_id, pool).await
}

pub async fn remove_all_pni(channel_id: &str, pool: &Pool) -> Result<u64, BitpartStoreError> {
    remove_all_impl("signal_pni_kyber_pre_keys", channel_id, pool).await
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
    max_key_id_impl("signal_kyber_pre_keys", channel_id, pool).await
}

pub async fn max_key_id_pni(
    channel_id: &str,
    pool: &Pool,
) -> Result<Option<u32>, BitpartStoreError> {
    max_key_id_impl("signal_pni_kyber_pre_keys", channel_id, pool).await
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
                "CREATE TABLE signal_kyber_pre_keys (
                    channel_id varchar NOT NULL,
                    key_id integer NOT NULL,
                    record_data blob NOT NULL,
                    is_last_resort integer NOT NULL DEFAULT 0,
                    PRIMARY KEY (channel_id, key_id)
                )",
                [],
            )?;
            c.execute(
                "CREATE TABLE signal_pni_kyber_pre_keys (
                    channel_id varchar NOT NULL,
                    key_id integer NOT NULL,
                    record_data blob NOT NULL,
                    is_last_resort integer NOT NULL DEFAULT 0,
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
        let record_data = b"test_kyber_pre_key_data";

        assert_eq!(get_aci(channel_id, key_id, &pool).await.unwrap(), None);

        set_aci(channel_id, key_id, record_data, false, &pool)
            .await
            .unwrap();

        let retrieved = get_aci(channel_id, key_id, &pool).await.unwrap();
        assert_eq!(retrieved, Some(record_data.to_vec()));

        let removed = remove_aci(channel_id, key_id, &pool).await.unwrap();
        assert_eq!(removed, Some(record_data.to_vec()));

        assert_eq!(get_aci(channel_id, key_id, &pool).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_get_all_with_last_resort() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";

        set_aci(channel_id, 1, b"data1", false, &pool)
            .await
            .unwrap();
        set_aci(channel_id, 2, b"data2", true, &pool).await.unwrap();
        set_aci(channel_id, 3, b"data3", false, &pool)
            .await
            .unwrap();

        let all_keys = get_all_aci(channel_id, &pool).await.unwrap();
        assert_eq!(all_keys.len(), 3);

        assert!(all_keys.contains(&(1u32, b"data1".to_vec(), false)));
        assert!(all_keys.contains(&(2u32, b"data2".to_vec(), true)));
        assert!(all_keys.contains(&(3u32, b"data3".to_vec(), false)));
    }

    #[tokio::test]
    async fn test_get_last_resort_only() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";

        set_aci(channel_id, 1, b"normal1", false, &pool)
            .await
            .unwrap();
        set_aci(channel_id, 2, b"last_resort2", true, &pool)
            .await
            .unwrap();
        set_aci(channel_id, 3, b"normal3", false, &pool)
            .await
            .unwrap();
        set_aci(channel_id, 4, b"last_resort4", true, &pool)
            .await
            .unwrap();

        let last_resort_keys = get_last_resort_aci(channel_id, &pool).await.unwrap();
        assert_eq!(last_resort_keys.len(), 2);

        assert!(last_resort_keys.contains(&(2u32, b"last_resort2".to_vec())));
        assert!(last_resort_keys.contains(&(4u32, b"last_resort4".to_vec())));
    }

    #[tokio::test]
    async fn test_max_key_id() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";

        assert_eq!(max_key_id_aci(channel_id, &pool).await.unwrap(), None);

        set_aci(channel_id, 7, b"data7", false, &pool)
            .await
            .unwrap();
        set_aci(channel_id, 3, b"data3", true, &pool).await.unwrap();
        set_aci(channel_id, 12, b"data12", false, &pool)
            .await
            .unwrap();

        let max_id = max_key_id_aci(channel_id, &pool).await.unwrap();
        assert_eq!(max_id, Some(12u32));
    }
}
