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
use serde_json::Value;
use uuid::Uuid;

fn pool_err(e: impl std::fmt::Display) -> BitpartErrorKind {
    BitpartErrorKind::Pool(e.to_string())
}

fn render_expires(expires_at: Option<NaiveDateTime>) -> Option<String> {
    expires_at.map(|e| e.to_string())
}

pub async fn get(
    client: &Client,
    r#type: &str,
    key: &str,
    db: &Pool,
) -> Result<Value> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let r#type = r#type.to_owned();
    let key = key.to_owned();

    let obj = db.get().await.map_err(pool_err)?;
    let row = obj
        .interact(move |conn| -> rusqlite::Result<Option<String>> {
            let mut stmt = conn.prepare(
                "SELECT value FROM state \
                 WHERE bot_id = ? AND channel_id = ? AND user_id = ? \
                   AND type = ? AND key = ? \
                 LIMIT 1",
            )?;
            stmt.query_row(
                params![bot_id, channel_id, user_id, r#type, key],
                |r| r.get::<_, String>(0),
            )
            .optional()
        })
        .await
        .map_err(pool_err)??;

    match row {
        Some(value) => Ok(serde_json::from_str(&value)?),
        None => Err(BitpartErrorKind::Interpreter("No state found".to_owned()).into()),
    }
}

pub async fn get_by_client(client: &Client, db: &Pool) -> Result<Vec<Value>> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();

    let obj = db.get().await.map_err(pool_err)?;
    let values = obj
        .interact(move |conn| -> rusqlite::Result<Vec<String>> {
            let mut stmt = conn.prepare(
                "SELECT value FROM state \
                 WHERE bot_id = ? AND channel_id = ? AND user_id = ?",
            )?;
            let rows = stmt.query_map(
                params![bot_id, channel_id, user_id],
                |r| r.get::<_, String>(0),
            )?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
        .map_err(pool_err)??;

    Ok(values.into_iter().map(Value::String).collect())
}

pub async fn set(
    client: &Client,
    r#type: &str,
    key: &str,
    value: &Value,
    expires_at: Option<NaiveDateTime>,
    db: &Pool,
) -> Result<()> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let type_ = r#type.to_owned();
    let key = key.to_owned();
    let value_str = value.to_string();
    let expires_at_str = render_expires(expires_at);

    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<()> {
        // Find existing row by (bot_id, channel_id, user_id, type, key).
        let existing_id: Option<String> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM state \
                 WHERE bot_id = ? AND channel_id = ? AND user_id = ? \
                   AND type = ? AND key = ? \
                 LIMIT 1",
            )?;
            stmt.query_row(
                params![bot_id, channel_id, user_id, type_, key],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        };

        match existing_id {
            None => {
                let new_id = Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO state \
                     (id, bot_id, channel_id, user_id, type, key, value, expires_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        new_id,
                        bot_id,
                        channel_id,
                        user_id,
                        type_,
                        key,
                        value_str,
                        expires_at_str,
                    ],
                )?;
            }
            Some(id) => {
                // Update value + expires_at. The AFTER UPDATE trigger
                // bumps `updated_at`.
                conn.execute(
                    "UPDATE state SET value = ?, expires_at = ? WHERE id = ?",
                    params![value_str, expires_at_str, id],
                )?;
            }
        }
        Ok(())
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn delete(
    client: &Client,
    r#type: &str,
    key: &str,
    db: &Pool,
) -> Result<()> {
    let bot_id = client.bot_id.clone();
    let channel_id = client.channel_id.clone();
    let user_id = client.user_id.clone();
    let type_ = r#type.to_owned();
    let key = key.to_owned();

    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<usize> {
        conn.execute(
            "DELETE FROM state \
             WHERE bot_id = ? AND channel_id = ? AND user_id = ? \
               AND type = ? AND key = ?",
            params![bot_id, channel_id, user_id, type_, key],
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
            "DELETE FROM state \
             WHERE bot_id = ? AND channel_id = ? AND user_id = ?",
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
        conn.execute("DELETE FROM state WHERE bot_id = ?", params![bot_id])
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}
