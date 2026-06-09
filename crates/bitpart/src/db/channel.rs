// Bitpart
// Copyright (C) 2025 Throneless Tech

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

use bitpart_common::db::Pool;
use bitpart_common::error::{BitpartErrorKind, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn pool_err(e: impl std::fmt::Display) -> BitpartErrorKind {
    BitpartErrorKind::Pool(e.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub bot_id: String,
    pub channel_id: String,
    pub updated_at: String,
    pub created_at: String,
}

fn row_to_model(r: &rusqlite::Row<'_>) -> rusqlite::Result<Model> {
    Ok(Model {
        id: r.get("id")?,
        bot_id: r.get("bot_id")?,
        channel_id: r.get("channel_id")?,
        updated_at: r.get("updated_at")?,
        created_at: r.get("created_at")?,
    })
}

pub async fn create(channel_id: &str, bot_id: &str, db: &Pool) -> Result<String> {
    let channel_id = channel_id.to_owned();
    let bot_id = bot_id.to_owned();

    let obj = db.get().await.map_err(pool_err)?;
    let id = obj
        .interact(move |conn| -> rusqlite::Result<String> {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM channel WHERE bot_id = ? AND channel_id = ? LIMIT 1",
                    params![bot_id, channel_id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(id) = existing {
                return Ok(id);
            }
            let new_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO channel (id, bot_id, channel_id) VALUES (?, ?, ?)",
                params![new_id, bot_id, channel_id],
            )?;
            Ok(new_id)
        })
        .await
        .map_err(pool_err)??;
    Ok(id)
}

pub async fn list(limit: Option<u64>, offset: Option<u64>, db: &Pool) -> Result<Vec<Model>> {
    let obj = db.get().await.map_err(pool_err)?;
    let rows = obj
        .interact(move |conn| -> rusqlite::Result<Vec<Model>> {
            let lim: i64 = limit.map(|n| n as i64).unwrap_or(-1);
            let off: i64 = offset.map(|n| n as i64).unwrap_or(0);
            let mut stmt = conn.prepare(
                "SELECT id, bot_id, channel_id, updated_at, created_at FROM channel \
                 ORDER BY created_at DESC \
                 LIMIT ? OFFSET ?",
            )?;
            let rows = stmt.query_map(params![lim, off], row_to_model)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
        .map_err(pool_err)??;
    Ok(rows)
}

pub async fn get(channel_id: &str, bot_id: &str, db: &Pool) -> Result<Option<Model>> {
    let channel_id = channel_id.to_owned();
    let bot_id = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let row = obj
        .interact(move |conn| -> rusqlite::Result<Option<Model>> {
            let mut stmt = conn.prepare(
                "SELECT id, bot_id, channel_id, updated_at, created_at FROM channel \
                 WHERE bot_id = ? AND channel_id = ? LIMIT 1",
            )?;
            stmt.query_row(params![bot_id, channel_id], row_to_model)
                .optional()
        })
        .await
        .map_err(pool_err)??;
    Ok(row)
}

pub async fn get_by_id(id: &str, db: &Pool) -> Result<Option<Model>> {
    let id = id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let row = obj
        .interact(move |conn| -> rusqlite::Result<Option<Model>> {
            let mut stmt = conn.prepare(
                "SELECT id, bot_id, channel_id, updated_at, created_at FROM channel \
                 WHERE id = ?",
            )?;
            stmt.query_row(params![id], row_to_model).optional()
        })
        .await
        .map_err(pool_err)??;
    Ok(row)
}

pub async fn get_by_bot_id(bot_id: &str, db: &Pool) -> Result<Vec<Model>> {
    let bot_id = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let rows = obj
        .interact(move |conn| -> rusqlite::Result<Vec<Model>> {
            let mut stmt = conn.prepare(
                "SELECT id, bot_id, channel_id, updated_at, created_at FROM channel \
                 WHERE bot_id = ?",
            )?;
            let rows = stmt.query_map(params![bot_id], row_to_model)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
        .map_err(pool_err)??;
    Ok(rows)
}

pub async fn delete(channel_id: &str, bot_id: &str, db: &Pool) -> Result<()> {
    let channel_id_owned = channel_id.to_owned();
    let bot_id_owned = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let affected = obj
        .interact(move |conn| -> rusqlite::Result<usize> {
            conn.execute(
                "DELETE FROM channel WHERE bot_id = ? AND channel_id = ?",
                params![bot_id_owned, channel_id_owned],
            )
        })
        .await
        .map_err(pool_err)??;
    if affected == 0 {
        Err(BitpartErrorKind::Api(format!("Record not found: {bot_id}")).into())
    } else {
        Ok(())
    }
}

pub async fn delete_by_bot_id(bot_id: &str, db: &Pool) -> Result<()> {
    let bot_id = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<usize> {
        conn.execute(
            "DELETE FROM channel WHERE id = (SELECT id FROM channel WHERE bot_id = ? LIMIT 1)",
            params![bot_id],
        )
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn delete_by_id(id: &str, db: &Pool) -> Result<()> {
    let id = id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<usize> {
        conn.execute("DELETE FROM channel WHERE id = ?", params![id])
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}
