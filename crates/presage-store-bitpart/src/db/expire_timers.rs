// presage-store-bitpart
// Copyright (C) 2025 Throneless Tech
//
// This code is derived in part from code from the Presage project:
// Copyright (C) 2024 Gabriel Féron

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

use deadpool_sqlite::Pool;
use rusqlite::{OptionalExtension, params};

use crate::error::BitpartStoreError;

fn pool_err(e: impl std::fmt::Display) -> BitpartStoreError {
    BitpartStoreError::Pool(e.to_string())
}

pub async fn get(
    channel_id: &str,
    thread_id: &str,
    pool: &Pool,
) -> Result<Option<(u32, u32)>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let thread_id = thread_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<(u32, u32)>> {
        c.query_row(
            "SELECT timer, version FROM signal_expire_timers \
             WHERE channel_id = ?1 AND thread_id = ?2",
            params![channel_id, thread_id],
            |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)),
        )
        .optional()
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn set_if_newer(
    channel_id: &str,
    thread_id: &str,
    timer: u32,
    version: u32,
    pool: &Pool,
) -> Result<(), BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let thread_id = thread_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO signal_expire_timers (channel_id, thread_id, timer, version) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(channel_id, thread_id) DO UPDATE SET \
                 timer = excluded.timer, version = excluded.version \
             WHERE excluded.version > signal_expire_timers.version",
            params![channel_id, thread_id, timer, version],
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn remove(
    channel_id: &str,
    thread_id: &str,
    pool: &Pool,
) -> Result<u64, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let thread_id = thread_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<u64> {
        let n = c.execute(
            "DELETE FROM signal_expire_timers WHERE channel_id = ?1 AND thread_id = ?2",
            params![channel_id, thread_id],
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
                "CREATE TABLE signal_expire_timers (
                    channel_id varchar NOT NULL,
                    thread_id varchar NOT NULL,
                    timer integer NOT NULL,
                    version integer NOT NULL,
                    PRIMARY KEY (channel_id, thread_id)
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

        assert_eq!(get("ch1", "thread1", &pool).await.unwrap(), None);

        set_if_newer("ch1", "thread1", 3600, 1, &pool)
            .await
            .unwrap();
        assert_eq!(get("ch1", "thread1", &pool).await.unwrap(), Some((3600, 1)));

        assert_eq!(remove("ch1", "thread1", &pool).await.unwrap(), 1);
        assert_eq!(get("ch1", "thread1", &pool).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_newer_version_wins() {
        let pool = setup_test_pool().await;

        set_if_newer("ch1", "thread1", 3600, 1, &pool)
            .await
            .unwrap();
        set_if_newer("ch1", "thread1", 604800, 2, &pool)
            .await
            .unwrap();
        assert_eq!(
            get("ch1", "thread1", &pool).await.unwrap(),
            Some((604800, 2))
        );
    }

    #[tokio::test]
    async fn test_older_or_equal_version_is_ignored() {
        let pool = setup_test_pool().await;

        set_if_newer("ch1", "thread1", 3600, 5, &pool)
            .await
            .unwrap();

        set_if_newer("ch1", "thread1", 0, 4, &pool).await.unwrap();
        assert_eq!(get("ch1", "thread1", &pool).await.unwrap(), Some((3600, 5)));

        set_if_newer("ch1", "thread1", 0, 5, &pool).await.unwrap();
        assert_eq!(get("ch1", "thread1", &pool).await.unwrap(), Some((3600, 5)));
    }

    #[tokio::test]
    async fn test_threads_and_channels_are_independent() {
        let pool = setup_test_pool().await;

        set_if_newer("ch1", "thread1", 3600, 1, &pool)
            .await
            .unwrap();
        set_if_newer("ch1", "thread2", 60, 1, &pool).await.unwrap();
        set_if_newer("ch2", "thread1", 120, 1, &pool).await.unwrap();

        assert_eq!(get("ch1", "thread1", &pool).await.unwrap(), Some((3600, 1)));
        assert_eq!(get("ch1", "thread2", &pool).await.unwrap(), Some((60, 1)));
        assert_eq!(get("ch2", "thread1", &pool).await.unwrap(), Some((120, 1)));

        remove("ch1", "thread1", &pool).await.unwrap();
        assert_eq!(get("ch1", "thread2", &pool).await.unwrap(), Some((60, 1)));
        assert_eq!(get("ch2", "thread1", &pool).await.unwrap(), Some((120, 1)));
    }
}
