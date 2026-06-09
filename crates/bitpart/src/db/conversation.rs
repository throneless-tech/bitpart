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
use chrono::NaiveDateTime;
use csml_interpreter::data::Client;
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
    pub user_id: String,
    pub flow_id: String,
    pub step_id: String,
    pub status: String,
    pub last_interaction_at: String,
    pub updated_at: String,
    pub created_at: String,
    pub expires_at: Option<String>,
}

const SELECT_COLS: &str = "id, bot_id, channel_id, user_id, flow_id, step_id, status, \
                          last_interaction_at, updated_at, created_at, expires_at";

fn row_to_model(r: &rusqlite::Row<'_>) -> rusqlite::Result<Model> {
    Ok(Model {
        id: r.get("id")?,
        bot_id: r.get("bot_id")?,
        channel_id: r.get("channel_id")?,
        user_id: r.get("user_id")?,
        flow_id: r.get("flow_id")?,
        step_id: r.get("step_id")?,
        status: r.get("status")?,
        last_interaction_at: r.get("last_interaction_at")?,
        updated_at: r.get("updated_at")?,
        created_at: r.get("created_at")?,
        expires_at: r.get("expires_at")?,
    })
}

pub async fn create(
    flow_id: &str,
    step_id: &str,
    client: &Client,
    expires_at: Option<NaiveDateTime>,
    db: &Pool,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let flow_id = flow_id.to_owned();
    let step_id = step_id.to_owned();
    let expires_at_str = expires_at.map(|e| e.to_string());

    let obj = db.get().await.map_err(pool_err)?;
    let id_clone = id.clone();
    obj.interact(move |conn| -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO conversation \
             (id, bot_id, channel_id, user_id, flow_id, step_id, status, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, 'OPEN', ?)",
            params![
                id_clone,
                bot_id,
                channel_id,
                user_id,
                flow_id,
                step_id,
                expires_at_str,
            ],
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)??;
    Ok(id)
}

pub async fn set_status_by_id(id: &str, status: &str, db: &Pool) -> Result<()> {
    let id = id.to_owned();
    let status = status.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<()> {
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM conversation WHERE id = ? LIMIT 1",
                params![id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if exists {
            conn.execute(
                "UPDATE conversation SET status = ? WHERE id = ?",
                params![status, id],
            )?;
        }
        Ok(())
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn set_status_by_client(client: &Client, status: &str, db: &Pool) -> Result<()> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let status = status.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<usize> {
        conn.execute(
            "UPDATE conversation SET status = ? \
             WHERE bot_id = ? AND channel_id = ? AND user_id = ?",
            params![status, bot_id, channel_id, user_id],
        )
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn get_latest_open_by_client(client: &Client, db: &Pool) -> Result<Option<Model>> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let obj = db.get().await.map_err(pool_err)?;
    let row = obj
        .interact(move |conn| -> rusqlite::Result<Option<Model>> {
            let sql = format!(
                "SELECT {SELECT_COLS} FROM conversation \
                 WHERE bot_id = ? AND channel_id = ? AND user_id = ? AND status = 'OPEN' \
                 ORDER BY created_at DESC LIMIT 1"
            );
            let mut stmt = conn.prepare(&sql)?;
            stmt.query_row(params![bot_id, channel_id, user_id], row_to_model)
                .optional()
        })
        .await
        .map_err(pool_err)??;
    Ok(row)
}

pub async fn get_by_client(
    client: &Client,
    limit: Option<u64>,
    offset: Option<u64>,
    db: &Pool,
) -> Result<Vec<Model>> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let obj = db.get().await.map_err(pool_err)?;
    let rows = obj
        .interact(move |conn| -> rusqlite::Result<Vec<Model>> {
            let lim: i64 = limit.map(|n| n as i64).unwrap_or(-1);
            let off: i64 = offset.map(|n| n as i64).unwrap_or(0);
            let sql = format!(
                "SELECT {SELECT_COLS} FROM conversation \
                 WHERE bot_id = ? AND channel_id = ? AND user_id = ? \
                 LIMIT ? OFFSET ?"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows =
                stmt.query_map(params![bot_id, channel_id, user_id, lim, off], row_to_model)?;
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

pub async fn get_open_by_bot_id(
    bot_id: &str,
    limit: Option<u64>,
    offset: Option<u64>,
    db: &Pool,
) -> Result<Vec<Model>> {
    let bot_id = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let rows = obj
        .interact(move |conn| -> rusqlite::Result<Vec<Model>> {
            let lim: i64 = limit.map(|n| n as i64).unwrap_or(-1);
            let off: i64 = offset.map(|n| n as i64).unwrap_or(0);
            let sql = format!(
                "SELECT {SELECT_COLS} FROM conversation \
                 WHERE bot_id = ? AND status = 'OPEN' \
                 LIMIT ? OFFSET ?"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![bot_id, lim, off], row_to_model)?;
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

pub async fn update(
    id: &str,
    flow_id: Option<String>,
    step_id: Option<String>,
    db: &Pool,
) -> Result<()> {
    if flow_id.is_none() && step_id.is_none() {
        return Ok(());
    }
    let id = id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<()> {
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM conversation WHERE id = ? LIMIT 1",
                params![id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if !exists {
            return Ok(());
        }
        match (flow_id, step_id) {
            (Some(f), Some(s)) => {
                conn.execute(
                    "UPDATE conversation SET flow_id = ?, step_id = ? WHERE id = ?",
                    params![f, s, id],
                )?;
            }
            (Some(f), None) => {
                conn.execute(
                    "UPDATE conversation SET flow_id = ? WHERE id = ?",
                    params![f, id],
                )?;
            }
            (None, Some(s)) => {
                conn.execute(
                    "UPDATE conversation SET step_id = ? WHERE id = ?",
                    params![s, id],
                )?;
            }
            (None, None) => {}
        }
        Ok(())
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn delete_by_client(client: &Client, db: &Pool) -> Result<()> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<usize> {
        conn.execute(
            "DELETE FROM conversation WHERE bot_id = ? AND channel_id = ? AND user_id = ?",
            params![bot_id, channel_id, user_id],
        )
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn delete_by_bot_id(bot_id: &str, db: &Pool) -> Result<()> {
    let bot_id = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<usize> {
        conn.execute("DELETE FROM conversation WHERE bot_id = ?", params![bot_id])
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}
