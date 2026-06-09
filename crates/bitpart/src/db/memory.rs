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
use csml_interpreter::data::{Client, Memory as CsmlMemory};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
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
    pub key: String,
    pub value: Value,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
}

const SELECT_COLS: &str =
    "id, bot_id, channel_id, user_id, key, value, created_at, updated_at, expires_at";

fn row_to_model(r: &rusqlite::Row<'_>) -> rusqlite::Result<Model> {
    let value_text: String = r.get("value")?;
    let value: Value = serde_json::from_str(&value_text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            5, // 0-indexed position of `value` in SELECT_COLS
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    Ok(Model {
        id: r.get("id")?,
        bot_id: r.get("bot_id")?,
        channel_id: r.get("channel_id")?,
        user_id: r.get("user_id")?,
        key: r.get("key")?,
        value,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        expires_at: r.get("expires_at")?,
    })
}

pub async fn create(
    client: &Client,
    key: &str,
    value: &Value,
    expires_at: Option<NaiveDateTime>,
    db: &Pool,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let key = key.to_owned();
    let value_str = value.to_string();
    let expires_at_str = expires_at.map(|e| e.to_string());

    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO memory \
             (id, bot_id, channel_id, user_id, key, value, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                id,
                bot_id,
                channel_id,
                user_id,
                key,
                value_str,
                expires_at_str,
            ],
        )?;
        Ok(())
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn create_many(
    client: &Client,
    memories: &HashMap<String, CsmlMemory>,
    expires_at: Option<NaiveDateTime>,
    db: &Pool,
) -> Result<()> {
    if memories.is_empty() {
        return Ok(());
    }
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let expires_at_str = expires_at.map(|e| e.to_string());
    // Materialise the inputs as owned (key, json_text) so we can send
    // them across the `interact` boundary.
    let entries: Vec<(String, String)> = memories
        .iter()
        .map(|(k, v)| (k.clone(), v.value.to_string()))
        .collect();

    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<()> {
        let mut to_insert: Vec<(String, String)> = Vec::new();
        for (key, value_str) in entries {
            let existing_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM memory \
                     WHERE bot_id = ? AND channel_id = ? AND user_id = ? AND key = ? \
                     LIMIT 1",
                    params![bot_id, channel_id, user_id, key],
                    |r| r.get(0),
                )
                .optional()?;
            match existing_id {
                Some(id) => {
                    conn.execute(
                        "UPDATE memory SET value = ? WHERE id = ?",
                        params![value_str, id],
                    )?;
                }
                None => to_insert.push((key, value_str)),
            }
        }
        if !to_insert.is_empty() {
            let mut sql = String::from(
                "INSERT INTO memory \
                 (id, bot_id, channel_id, user_id, key, value, expires_at) VALUES ",
            );
            let mut params_vec: Vec<rusqlite::types::Value> = Vec::new();
            for (i, (key, value_str)) in to_insert.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push_str("(?, ?, ?, ?, ?, ?, ?)");
                let new_id = Uuid::new_v4().to_string();
                params_vec.push(new_id.into());
                params_vec.push(bot_id.clone().into());
                params_vec.push(channel_id.clone().into());
                params_vec.push(user_id.clone().into());
                params_vec.push(key.clone().into());
                params_vec.push(value_str.clone().into());
                params_vec.push(match &expires_at_str {
                    Some(s) => s.clone().into(),
                    None => rusqlite::types::Value::Null,
                });
            }
            conn.execute(&sql, rusqlite::params_from_iter(params_vec))?;
        }
        Ok(())
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn get(client: &Client, key: &str, db: &Pool) -> Result<Option<Model>> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let key = key.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let row = obj
        .interact(move |conn| -> rusqlite::Result<Option<Model>> {
            let sql = format!(
                "SELECT {SELECT_COLS} FROM memory \
                 WHERE bot_id = ? AND channel_id = ? AND user_id = ? AND key = ? LIMIT 1"
            );
            let mut stmt = conn.prepare(&sql)?;
            stmt.query_row(params![bot_id, channel_id, user_id, key], row_to_model)
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
                "SELECT {SELECT_COLS} FROM memory \
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

pub async fn get_by_memory(key: &str, bot_id: &str, db: &Pool) -> Result<Vec<Model>> {
    let key = key.to_owned();
    let bot_id = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let rows = obj
        .interact(move |conn| -> rusqlite::Result<Vec<Model>> {
            let sql = format!("SELECT {SELECT_COLS} FROM memory WHERE key = ? AND bot_id = ?");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![key, bot_id], row_to_model)?;
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

pub async fn delete(client: &Client, key: &str, db: &Pool) -> Result<()> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let key = key.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<usize> {
        conn.execute(
            "DELETE FROM memory \
             WHERE bot_id = ? AND channel_id = ? AND user_id = ? AND key = ?",
            params![bot_id, channel_id, user_id, key],
        )
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
            "DELETE FROM memory WHERE bot_id = ? AND channel_id = ? AND user_id = ?",
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
        conn.execute("DELETE FROM memory WHERE bot_id = ?", params![bot_id])
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}
