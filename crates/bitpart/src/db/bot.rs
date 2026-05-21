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

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use bitpart_common::db::Pool;
use bitpart_common::error::{BitpartErrorKind, Result};
use csml_interpreter::data::{CsmlBot, CsmlFlow, Module, MultiBot};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::env;
use uuid::Uuid;

use crate::csml::data::BotVersion;

fn pool_err(e: impl std::fmt::Display) -> BitpartErrorKind {
    BitpartErrorKind::Pool(e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedCsmlBot {
    pub id: String,
    pub name: String,
    pub flows: Vec<CsmlFlow>,
    pub native_components: Option<String>,
    pub custom_components: Option<String>,
    pub default_flow: String,
    pub no_interruption_delay: Option<i32>,
    pub env: Option<String>,
    pub modules: Option<Vec<Module>>,
    pub apps_endpoint: Option<String>,
    pub multibot: Option<Vec<MultiBot>>,
}

impl From<SerializedCsmlBot> for CsmlBot {
    fn from(val: SerializedCsmlBot) -> Self {
        CsmlBot {
            id: val.id.clone(),
            name: val.name.clone(),
            apps_endpoint: val.apps_endpoint,
            flows: val.flows.clone(),
            native_components: match val.native_components {
                Some(value) => match serde_json::from_str(&value) {
                    Ok(serde_json::Value::Object(map)) => Some(map),
                    _ => unreachable!(),
                },
                None => None,
            },
            custom_components: match val.custom_components {
                Some(value) => match serde_json::from_str(&value) {
                    Ok(value) => Some(value),
                    Err(_) => unreachable!(),
                },
                None => None,
            },
            default_flow: val.default_flow.clone(),
            bot_ast: None,
            no_interruption_delay: val.no_interruption_delay,
            env: val
                .env
                .as_ref()
                .map(|e| serde_json::from_str(e).unwrap_or(serde_json::Value::Null)),
            modules: val.modules.clone(),
            multibot: val.multibot,
        }
    }
}

#[allow(dead_code)]
struct BotRow {
    id: String,
    bot_id: String,
    bot_json: String,
}

impl BotRow {
    fn into_version_row_id(self) -> Result<BotVersion> {
        let bot: SerializedCsmlBot = serde_json::from_str(&self.bot_json)?;
        let row_id = self.id;
        Ok(BotVersion {
            version_id: row_id,
            bot: bot.into(),
            engine_version: env!("CARGO_PKG_VERSION").to_owned(),
        })
    }

    fn into_version_bot_id(self) -> Result<BotVersion> {
        let bot: SerializedCsmlBot = serde_json::from_str(&self.bot_json)?;
        Ok(BotVersion {
            version_id: bot.id.clone(),
            bot: bot.into(),
            engine_version: env!("CARGO_PKG_VERSION").to_owned(),
        })
    }
}

pub async fn list(
    limit: Option<u64>,
    offset: Option<u64>,
    db: &Pool,
) -> Result<Vec<String>> {
    let obj = db.get().await.map_err(pool_err)?;
    let res = obj
        .interact(move |conn| -> rusqlite::Result<Vec<String>> {
            let lim: i64 = limit.map(|n| n as i64).unwrap_or(-1);
            let off: i64 = offset.map(|n| n as i64).unwrap_or(0);
            let mut stmt = conn.prepare(
                "SELECT bot_id FROM bot \
                 GROUP BY bot_id \
                 ORDER BY created_at DESC \
                 LIMIT ? OFFSET ?",
            )?;
            let rows = stmt.query_map(params![lim, off], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
        .map_err(pool_err)??;
    Ok(res)
}

pub async fn get(
    bot_id: &str,
    limit: Option<u64>,
    offset: Option<u64>,
    db: &Pool,
) -> Result<Vec<BotVersion>> {
    let bot_id = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let rows = obj
        .interact(move |conn| -> rusqlite::Result<Vec<BotRow>> {
            let lim: i64 = limit.map(|n| n as i64).unwrap_or(-1);
            let off: i64 = offset.map(|n| n as i64).unwrap_or(0);
            let mut stmt = conn.prepare(
                "SELECT id, bot_id, bot FROM bot \
                 WHERE bot_id = ? \
                 ORDER BY updated_at DESC \
                 LIMIT ? OFFSET ?",
            )?;
            let rows = stmt.query_map(params![bot_id, lim, off], |r| {
                Ok(BotRow {
                    id: r.get(0)?,
                    bot_id: r.get(1)?,
                    bot_json: r.get(2)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
        .map_err(pool_err)??;

    Ok(rows
        .into_iter()
        .filter_map(|r| r.into_version_row_id().ok())
        .collect())
}

pub async fn get_by_id(id: &str, db: &Pool) -> Result<Option<BotVersion>> {
    let id = id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let row = obj
        .interact(move |conn| -> rusqlite::Result<Option<BotRow>> {
            let mut stmt = conn.prepare("SELECT id, bot_id, bot FROM bot WHERE id = ?")?;
            let row = stmt
                .query_row(params![id], |r| {
                    Ok(BotRow {
                        id: r.get(0)?,
                        bot_id: r.get(1)?,
                        bot_json: r.get(2)?,
                    })
                })
                .optional()?;
            Ok(row)
        })
        .await
        .map_err(pool_err)??;

    match row {
        Some(r) => Ok(Some(r.into_version_bot_id()?)),
        None => Ok(None),
    }
}

pub async fn get_latest_by_bot_id(
    bot_id: &str,
    db: &Pool,
) -> Result<Option<BotVersion>> {
    let bot_id = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let row = obj
        .interact(move |conn| -> rusqlite::Result<Option<BotRow>> {
            let mut stmt = conn.prepare(
                "SELECT id, bot_id, bot FROM bot \
                 WHERE bot_id = ? \
                 ORDER BY updated_at DESC \
                 LIMIT 1",
            )?;
            let row = stmt
                .query_row(params![bot_id], |r| {
                    Ok(BotRow {
                        id: r.get(0)?,
                        bot_id: r.get(1)?,
                        bot_json: r.get(2)?,
                    })
                })
                .optional()?;
            Ok(row)
        })
        .await
        .map_err(pool_err)??;

    match row {
        Some(r) => Ok(Some(r.into_version_bot_id()?)),
        None => Ok(None),
    }
}

// =====================================================================
// Write functions
// =====================================================================

pub async fn create(bot: CsmlBot, db: &Pool) -> Result<BotVersion> {
    let row_id = Uuid::new_v4().to_string();
    let bot_id = bot.id.clone();
    let bot_json = bot.to_json().to_string();
    let engine_version = env!("CARGO_PKG_VERSION").to_owned();

    let obj = db.get().await.map_err(pool_err)?;
    let inserted_json = {
        let row_id = row_id.clone();
        let engine_version = engine_version.clone();
        obj.interact(move |conn| -> rusqlite::Result<String> {
            // Explicit column list — matches the migration order and
            // future-proofs against schema drift. `created_at`/`updated_at`
            // get their `CURRENT_TIMESTAMP` defaults.
            conn.execute(
                "INSERT INTO bot (id, bot_id, bot, engine_version) VALUES (?, ?, ?, ?)",
                params![row_id, bot_id, bot_json, engine_version],
            )?;
            Ok(bot_json)
        })
        .await
        .map_err(pool_err)??
    };

    let serialised: SerializedCsmlBot = serde_json::from_str(&inserted_json)?;
    Ok(BotVersion {
        bot: serialised.into(),
        version_id: row_id,
        engine_version,
    })
}

pub async fn touch(id: &str, version_id: &str, db: &Pool) -> Result<Option<BotVersion>> {
    let id = id.to_owned();
    let version_id = version_id.to_owned();

    let obj = db.get().await.map_err(pool_err)?;
    let row = obj
        .interact(move |conn| -> rusqlite::Result<Option<BotRow>> {
            let mut stmt = conn.prepare(
                "SELECT id, bot_id, bot FROM bot WHERE id = ? AND bot_id = ?",
            )?;
            let row = stmt
                .query_row(params![version_id, id], |r| {
                    Ok(BotRow {
                        id: r.get(0)?,
                        bot_id: r.get(1)?,
                        bot_json: r.get(2)?,
                    })
                })
                .optional()?;
            if let Some(ref r) = row {
                // Fire the AFTER UPDATE trigger. The SET is a no-op on
                // column values; SQLite still treats the row as updated.
                conn.execute(
                    "UPDATE bot SET engine_version = engine_version WHERE id = ?",
                    params![r.id],
                )?;
            }
            Ok(row)
        })
        .await
        .map_err(pool_err)??;

    match row {
        Some(r) => {
            Ok(Some(r.into_version_row_id()?))
        }
        None => Ok(None),
    }
}

pub async fn delete_by_bot_id(bot_id: &str, db: &Pool) -> Result<()> {
    let bot_id_owned = bot_id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    let affected = obj
        .interact(move |conn| -> rusqlite::Result<usize> {
            conn.execute("DELETE FROM bot WHERE bot_id = ?", params![bot_id_owned])
        })
        .await
        .map_err(pool_err)??;
    if affected == 0 {
        Err(BitpartErrorKind::Api(format!(
            "Record not found: bot_id={bot_id}"
        ))
        .into())
    } else {
        Ok(())
    }
}

pub async fn delete_by_id(id: &str, db: &Pool) -> Result<()> {
    let id = id.to_owned();
    let obj = db.get().await.map_err(pool_err)?;
    obj.interact(move |conn| -> rusqlite::Result<usize> {
        conn.execute("DELETE FROM bot WHERE id = ?", params![id])
    })
    .await
    .map_err(pool_err)??;
    Ok(())
}
