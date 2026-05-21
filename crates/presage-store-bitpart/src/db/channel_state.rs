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
use uuid;

use crate::error::BitpartStoreError;

fn pool_err(e: impl std::fmt::Display) -> BitpartStoreError {
    BitpartStoreError::Pool(e.to_string())
}

pub async fn get(
    channel_id: &str,
    tree: &str,
    key: &str,
    pool: &Pool,
) -> Result<Option<String>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let tree = tree.to_owned();
    let key = key.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<String>> {
        let mut stmt = c.prepare(
            "SELECT value FROM channel_state \
             WHERE channel_id = ?1 AND tree = ?2 AND key = ?3 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![channel_id, tree, key])?;
        if let Some(r) = rows.next()? {
            Ok(Some(r.get(0)?))
        } else {
            Ok(None)
        }
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_all(
    channel_id: &str,
    tree: &str,
    pool: &Pool,
) -> Result<Vec<(String, String)>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let tree = tree.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = c.prepare(
            "SELECT key, value FROM channel_state \
             WHERE channel_id = ?1 AND tree = ?2",
        )?;
        let rows = stmt
            .query_map(params![channel_id, tree], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn get_trees(
    channel_id: &str,
    pool: &Pool,
) -> Result<Vec<String>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Vec<String>> {
        let mut stmt = c.prepare(
            "SELECT tree FROM channel_state \
             WHERE channel_id = ?1 GROUP BY tree",
        )?;
        let rows = stmt
            .query_map(params![channel_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

// Returns true on update, false on insert. Find-then-write, not atomic.
pub async fn set<V: Into<String>>(
    channel_id: &str,
    tree: &str,
    key: &str,
    value: V,
    pool: &Pool,
) -> Result<bool, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let tree = tree.to_owned();
    let key = key.to_owned();
    let value: String = value.into();
    conn.interact(move |c| -> rusqlite::Result<bool> {
        use rusqlite::OptionalExtension;
        let existing: Option<String> = c
            .query_row(
                "SELECT id FROM channel_state \
                 WHERE channel_id = ?1 AND tree = ?2 AND key = ?3 LIMIT 1",
                params![channel_id, tree, key],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        if let Some(id) = existing {
            c.execute(
                "UPDATE channel_state SET value = ?1 WHERE id = ?2",
                params![value, id],
            )?;
            Ok(true)
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            c.execute(
                "INSERT INTO channel_state (id, channel_id, tree, key, value) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, channel_id, tree, key, value],
            )?;
            Ok(false)
        }
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn remove(
    channel_id: &str,
    tree: &str,
    key: &str,
    pool: &Pool,
) -> Result<Option<String>, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let tree = tree.to_owned();
    let key = key.to_owned();
    conn.interact(move |c| -> rusqlite::Result<Option<String>> {
        let row: Option<(String, String)> = c
            .query_row(
                "SELECT id, value FROM channel_state \
                 WHERE channel_id = ?1 AND tree = ?2 AND key = ?3 LIMIT 1",
                params![channel_id, tree, key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        if let Some((id, value)) = row {
            c.execute("DELETE FROM channel_state WHERE id = ?1", params![id])?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn remove_all(
    channel_id: &str,
    tree: &str,
    pool: &Pool,
) -> Result<u64, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let tree = tree.to_owned();
    conn.interact(move |c| -> rusqlite::Result<u64> {
        let n = c.execute(
            "DELETE FROM channel_state WHERE channel_id = ?1 AND tree = ?2",
            params![channel_id, tree],
        )?;
        Ok(n as u64)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}

pub async fn remove_like(
    channel_id: &str,
    tree: &str,
    key: &str,
    pool: &Pool,
) -> Result<u64, BitpartStoreError> {
    let conn = pool.get().await.map_err(pool_err)?;
    let channel_id = channel_id.to_owned();
    let tree = tree.to_owned();
    let key = key.to_owned();
    conn.interact(move |c| -> rusqlite::Result<u64> {
        let n = c.execute(
            "DELETE FROM channel_state \
             WHERE channel_id = ?1 AND tree = ?2 AND key LIKE ?3",
            params![channel_id, tree, key],
        )?;
        Ok(n as u64)
    })
    .await
    .map_err(pool_err)?
    .map_err(BitpartStoreError::from)
}
