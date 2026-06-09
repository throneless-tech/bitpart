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
use rusqlite::{params, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::csml::data::ConversationData;

fn pool_err(e: impl std::fmt::Display) -> BitpartErrorKind {
    BitpartErrorKind::Pool(e.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub conversation_id: String,
    pub flow_id: String,
    pub step_id: String,
    pub direction: String,
    pub payload: String,
    pub content_type: String,
    pub message_order: i32,
    pub interaction_order: i32,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
}

const SELECT_COLS: &str = "id, conversation_id, flow_id, step_id, direction, payload, \
                          content_type, message_order, interaction_order, \
                          created_at, updated_at, expires_at";

fn row_to_model(r: &rusqlite::Row<'_>) -> rusqlite::Result<Model> {
    Ok(Model {
        id: r.get("id")?,
        conversation_id: r.get("conversation_id")?,
        flow_id: r.get("flow_id")?,
        step_id: r.get("step_id")?,
        direction: r.get("direction")?,
        payload: r.get("payload")?,
        content_type: r.get("content_type")?,
        message_order: r.get("message_order")?,
        interaction_order: r.get("interaction_order")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        expires_at: r.get("expires_at")?,
    })
}

pub async fn create(
    data: &ConversationData,
    messages: &[Value],
    interaction_order: i32,
    direction: &str,
    expires_at: Option<NaiveDateTime>,
    db: &Pool,
) -> Result<()> {
    if messages.is_empty() {
        return Ok(());
    }
    let conversation_id = data.conversation_id.clone();
    let flow_id = data.context.flow.clone();
    let step_id = data.context.step.get_step_ref().to_owned();
    let direction = direction.to_owned();
    let expires_at_str = expires_at.map(|e| e.to_string());

    // Materialise (payload_text, content_type_text) per message before
    // crossing the `interact` boundary.
    let prepared: Vec<(String, String)> = messages
        .iter()
        .map(|m| (m.to_string(), m["content_type"].to_string()))
        .collect();

    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<()> {
        let mut sql = String::from(
            "INSERT INTO message \
             (id, conversation_id, flow_id, step_id, direction, payload, content_type, \
              message_order, interaction_order, expires_at) VALUES ",
        );
        let mut params_vec: Vec<SqlValue> = Vec::new();
        for (i, (payload, content_type)) in prepared.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)");
            params_vec.push(Uuid::new_v4().to_string().into());
            params_vec.push(conversation_id.clone().into());
            params_vec.push(flow_id.clone().into());
            params_vec.push(step_id.clone().into());
            params_vec.push(direction.clone().into());
            params_vec.push(payload.clone().into());
            params_vec.push(content_type.clone().into());
            params_vec.push((i as i64).into());
            params_vec.push((interaction_order as i64).into());
            params_vec.push(match &expires_at_str {
                Some(s) => s.clone().into(),
                None => SqlValue::Null,
            });
        }
        conn.execute(&sql, rusqlite::params_from_iter(params_vec))?;
        Ok(())
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn delete_by_client(client: &Client, db: &Pool) -> Result<()> {
    let convos = super::conversation::get_by_client(client, None, None, db).await?;
    if convos.is_empty() {
        return Ok(());
    }
    let convo_ids: Vec<String> = convos.into_iter().map(|c| c.id).collect();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<()> {
        for id in convo_ids {
            conn.execute("DELETE FROM message WHERE conversation_id = ?", params![id])?;
        }
        Ok(())
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}

pub async fn get_by_client(
    client: &Client,
    limit: Option<u64>,
    offset: Option<u64>,
    db: &Pool,
) -> Result<Vec<Model>> {
    let convos = super::conversation::get_by_client(client, limit, offset, db).await?;
    if convos.is_empty() {
        return Ok(Vec::new());
    }
    let convo_ids: Vec<String> = convos.into_iter().map(|c| c.id).collect();
    let obj = db.get().await.map_err(pool_err)?;
    let rows = obj
        .interact(move |conn| -> rusqlite::Result<Vec<Model>> {
            let lim: i64 = limit.map(|n| n as i64).unwrap_or(-1);
            let off: i64 = offset.map(|n| n as i64).unwrap_or(0);
            let sql = format!(
                "SELECT {SELECT_COLS} FROM message \
                 WHERE conversation_id = ? LIMIT ? OFFSET ?"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut out = Vec::new();
            for id in convo_ids {
                let rows = stmt.query_map(params![id, lim, off], row_to_model)?;
                for row in rows {
                    out.push(row?);
                }
            }
            Ok(out)
        })
        .await
        .map_err(pool_err)??;
    Ok(rows)
}
