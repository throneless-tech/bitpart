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
    thread_id: &str,
    timestamp: i64,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let thread_id = thread_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        c.query_row(
            "SELECT content_data FROM signal_messages WHERE channel_id = ?1 AND thread_id = ?2 AND timestamp = ?3",
            params![channel_id, thread_id, timestamp],
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
    thread_id: &str,
    timestamp: i64,
    content_data: &[u8],
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let thread_id = thread_id.to_owned();
    let content_data = content_data.to_vec();
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_messages (channel_id, thread_id, timestamp, content_data) VALUES (?1, ?2, ?3, ?4) 
             ON CONFLICT(channel_id, thread_id, timestamp) DO UPDATE SET content_data = excluded.content_data",
            params![channel_id, thread_id, timestamp, content_data]
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_all(
    channel_id: &str,
    thread_id: &str,
    pool: &Pool,
) -> Result<Vec<(i64, Vec<u8>)>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let thread_id = thread_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Vec<(i64, Vec<u8>)>> {
        let mut stmt = c.prepare(
            "SELECT timestamp, content_data FROM signal_messages 
             WHERE channel_id = ?1 AND thread_id = ?2 
             ORDER BY timestamp",
        )?;
        let rows = stmt
            .query_map(params![channel_id, thread_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_range(
    channel_id: &str,
    thread_id: &str,
    start_timestamp: i64,
    end_timestamp: i64,
    pool: &Pool,
) -> Result<Vec<(i64, Vec<u8>)>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let thread_id = thread_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Vec<(i64, Vec<u8>)>> {
        let mut stmt = c.prepare(
            "SELECT timestamp, content_data FROM signal_messages 
             WHERE channel_id = ?1 AND thread_id = ?2 AND timestamp BETWEEN ?3 AND ?4 
             ORDER BY timestamp",
        )?;
        let rows = stmt
            .query_map(
                params![channel_id, thread_id, start_timestamp, end_timestamp],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn remove(
    channel_id: &str,
    thread_id: &str,
    timestamp: i64,
    pool: &Pool,
) -> Result<Option<Vec<u8>>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let thread_id = thread_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<Vec<u8>>> {
        let content_data: Option<Vec<u8>> = c
            .query_row(
                "SELECT content_data FROM signal_messages WHERE channel_id = ?1 AND thread_id = ?2 AND timestamp = ?3",
                params![channel_id, thread_id, timestamp],
                |row| row.get::<_, Vec<u8>>(0)
            )
            .optional()?;

        if content_data.is_some() {
            c.execute(
                "DELETE FROM signal_messages WHERE channel_id = ?1 AND thread_id = ?2 AND timestamp = ?3",
                params![channel_id, thread_id, timestamp]
            )?;
        }

        Ok(content_data)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn clear_thread(
    channel_id: &str,
    thread_id: &str,
    pool: &Pool,
) -> Result<u64, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let thread_id = thread_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<u64> {
        let n = c.execute(
            "DELETE FROM signal_messages WHERE channel_id = ?1 AND thread_id = ?2",
            params![channel_id, thread_id],
        )?;
        Ok(n as u64)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn clear_all_messages(channel_id: &str, pool: &Pool) -> Result<u64, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<u64> {
        let n = c.execute(
            "DELETE FROM signal_messages WHERE channel_id = ?1",
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
                "CREATE TABLE signal_messages (
                    channel_id varchar NOT NULL,
                    thread_id varchar NOT NULL,
                    timestamp integer NOT NULL,
                    content_data blob NOT NULL,
                    PRIMARY KEY (channel_id, thread_id, timestamp)
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
        let thread_id = "test_thread";
        let timestamp = 1234567890i64;
        let content_data = b"test_message_content";

        assert_eq!(
            get(channel_id, thread_id, timestamp, &pool).await.unwrap(),
            None
        );

        set(channel_id, thread_id, timestamp, content_data, &pool)
            .await
            .unwrap();

        let retrieved = get(channel_id, thread_id, timestamp, &pool).await.unwrap();
        assert_eq!(retrieved, Some(content_data.to_vec()));

        let removed = remove(channel_id, thread_id, timestamp, &pool)
            .await
            .unwrap();
        assert_eq!(removed, Some(content_data.to_vec()));

        assert_eq!(
            get(channel_id, thread_id, timestamp, &pool).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn test_get_all_ordered() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let thread_id = "test_thread";

        set(channel_id, thread_id, 3000, b"msg3", &pool)
            .await
            .unwrap();
        set(channel_id, thread_id, 1000, b"msg1", &pool)
            .await
            .unwrap();
        set(channel_id, thread_id, 2000, b"msg2", &pool)
            .await
            .unwrap();

        let all_messages = get_all(channel_id, thread_id, &pool).await.unwrap();
        assert_eq!(all_messages.len(), 3);

        assert_eq!(all_messages[0], (1000i64, b"msg1".to_vec()));
        assert_eq!(all_messages[1], (2000i64, b"msg2".to_vec()));
        assert_eq!(all_messages[2], (3000i64, b"msg3".to_vec()));
    }

    #[tokio::test]
    async fn test_get_range() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let thread_id = "test_thread";

        set(channel_id, thread_id, 1000, b"msg1", &pool)
            .await
            .unwrap();
        set(channel_id, thread_id, 2000, b"msg2", &pool)
            .await
            .unwrap();
        set(channel_id, thread_id, 3000, b"msg3", &pool)
            .await
            .unwrap();
        set(channel_id, thread_id, 4000, b"msg4", &pool)
            .await
            .unwrap();

        let range_messages = get_range(channel_id, thread_id, 1500, 3500, &pool)
            .await
            .unwrap();
        assert_eq!(range_messages.len(), 2);
        assert_eq!(range_messages[0], (2000i64, b"msg2".to_vec()));
        assert_eq!(range_messages[1], (3000i64, b"msg3".to_vec()));
    }

    #[tokio::test]
    async fn test_clear_thread() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";

        set(channel_id, "thread1", 1000, b"msg1", &pool)
            .await
            .unwrap();
        set(channel_id, "thread1", 2000, b"msg2", &pool)
            .await
            .unwrap();
        set(channel_id, "thread2", 1000, b"msg3", &pool)
            .await
            .unwrap();

        let cleared_count = clear_thread(channel_id, "thread1", &pool).await.unwrap();
        assert_eq!(cleared_count, 2);

        let remaining_thread1 = get_all(channel_id, "thread1", &pool).await.unwrap();
        assert_eq!(remaining_thread1.len(), 0);

        let remaining_thread2 = get_all(channel_id, "thread2", &pool).await.unwrap();
        assert_eq!(remaining_thread2.len(), 1);
        assert_eq!(remaining_thread2[0], (1000i64, b"msg3".to_vec()));
    }

    #[tokio::test]
    async fn test_clear_all_messages() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";

        set(channel_id, "thread1", 1000, b"msg1", &pool)
            .await
            .unwrap();
        set(channel_id, "thread2", 2000, b"msg2", &pool)
            .await
            .unwrap();

        let cleared_count = clear_all_messages(channel_id, &pool).await.unwrap();
        assert_eq!(cleared_count, 2);

        let remaining_thread1 = get_all(channel_id, "thread1", &pool).await.unwrap();
        let remaining_thread2 = get_all(channel_id, "thread2", &pool).await.unwrap();
        assert_eq!(remaining_thread1.len(), 0);
        assert_eq!(remaining_thread2.len(), 0);
    }

    #[tokio::test]
    async fn test_upsert_behavior() {
        let pool = setup_test_pool().await;
        let channel_id = "test_channel";
        let thread_id = "test_thread";
        let timestamp = 1234567890i64;

        set(channel_id, thread_id, timestamp, b"content1", &pool)
            .await
            .unwrap();
        set(channel_id, thread_id, timestamp, b"content2", &pool)
            .await
            .unwrap();

        let retrieved = get(channel_id, thread_id, timestamp, &pool).await.unwrap();
        assert_eq!(retrieved, Some(b"content2".to_vec()));
    }
}
