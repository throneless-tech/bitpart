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

use bitpart_common::error::Result;
use csml_interpreter::data::{CsmlBot, CsmlFlow, Module, MultiBot};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use std::env;
use uuid;

use super::entities::{prelude::*, *};
use crate::csml::data::BotVersion;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedCsmlBot {
    pub id: String,
    pub name: String,
    pub flows: Vec<CsmlFlow>,
    pub native_components: Option<String>, // serde_json::Map<String, serde_json::Value>
    pub custom_components: Option<String>, // serde_json::Value
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
            id: val.id.to_owned(),
            name: val.name.to_owned(),
            apps_endpoint: val.apps_endpoint,
            flows: val.flows.to_owned(),
            native_components: {
                match val.native_components.to_owned() {
                    Some(value) => match serde_json::from_str(&value) {
                        Ok(serde_json::Value::Object(map)) => Some(map),
                        _ => unreachable!(),
                    },
                    None => None,
                }
            },
            custom_components: {
                match val.custom_components.to_owned() {
                    Some(value) => match serde_json::from_str(&value) {
                        Ok(value) => Some(value),
                        Err(_e) => unreachable!(),
                    },
                    None => None,
                }
            },
            default_flow: val.default_flow.to_owned(),
            bot_ast: None,
            no_interruption_delay: val.no_interruption_delay,
            env: val
                .env
                .as_ref()
                .map(|e| serde_json::from_str(e).unwrap_or(JsonValue::Null)),
            modules: val.modules.to_owned(),
            multibot: val.multibot,
        }
    }
}

pub async fn create(bot: CsmlBot, db: &DatabaseConnection) -> Result<BotVersion> {
    let model = bot::ActiveModel {
        id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
        bot_id: ActiveValue::Set(bot.id.to_owned()),
        bot: ActiveValue::Set(bot.to_json().to_string()),
        engine_version: ActiveValue::Set(env!["CARGO_PKG_VERSION"].to_owned()),
        ..Default::default()
    };

    let entry = model.insert(db).await?;

    let bot: SerializedCsmlBot = serde_json::from_str(&entry.bot)?;

    Ok(BotVersion {
        bot: bot.into(),
        version_id: entry.id,
        engine_version: env!["CARGO_PKG_VERSION"].to_owned(),
    })
}

pub async fn list(
    limit: Option<u64>,
    offset: Option<u64>,
    db: &DatabaseConnection,
) -> Result<Vec<String>> {
    let entries = Bot::find()
        .column(bot::Column::BotId)
        .group_by(bot::Column::BotId)
        .order_by(bot::Column::CreatedAt, Order::Desc)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?;

    Ok(entries.into_iter().map(|e| e.bot_id.to_string()).collect())
}

pub async fn get(
    bot_id: &str,
    limit: Option<u64>,
    offset: Option<u64>,
    db: &DatabaseConnection,
) -> Result<Vec<BotVersion>> {
    let entries = Bot::find()
        .filter(bot::Column::BotId.eq(bot_id))
        .order_by(bot::Column::UpdatedAt, Order::Desc)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?;

    Ok(entries
        .into_iter()
        .filter_map(|e| {
            let bot: SerializedCsmlBot = serde_json::from_str(&e.bot).ok()?;
            Some(BotVersion {
                version_id: e.id.to_string(),
                bot: bot.into(),
                engine_version: env!["CARGO_PKG_VERSION"].to_owned(),
            })
        })
        .collect())
}

pub async fn get_by_id(id: &str, db: &DatabaseConnection) -> Result<Option<BotVersion>> {
    let entry = Bot::find_by_id(id).one(db).await?;
    match entry {
        Some(e) => {
            let bot: SerializedCsmlBot = serde_json::from_str(&e.bot)?;

            Ok(Some(BotVersion {
                version_id: bot.id.to_string(),
                bot: bot.into(),
                engine_version: env!["CARGO_PKG_VERSION"].to_owned(),
            }))
        }
        None => Ok(None),
    }
}

pub async fn touch(
    id: &str,
    version_id: &str,
    db: &DatabaseConnection,
) -> Result<Option<BotVersion>> {
    if let Some(entry) = Bot::find()
        .filter(bot::Column::Id.eq(version_id))
        .filter(bot::Column::BotId.eq(id))
        .one(db)
        .await?
    {
        let version_id = entry.id.clone();
        let bot: SerializedCsmlBot = serde_json::from_str(&entry.bot)?;

        let entry: bot::ActiveModel = entry.into();
        entry.update(db).await?;

        Ok(Some(BotVersion {
            version_id,
            bot: bot.into(),
            engine_version: env!["CARGO_PKG_VERSION"].to_owned(),
        }))
    } else {
        Ok(None)
    }
}

pub async fn get_latest_by_bot_id(
    bot_id: &str,
    db: &DatabaseConnection,
) -> Result<Option<BotVersion>> {
    let entry = Bot::find()
        .filter(bot::Column::BotId.eq(bot_id))
        .order_by(bot::Column::UpdatedAt, Order::Desc)
        .one(db)
        .await?;

    match entry {
        Some(e) => {
            let bot: SerializedCsmlBot = serde_json::from_str(&e.bot)?;

            Ok(Some(BotVersion {
                version_id: bot.id.to_string(),
                bot: bot.into(),
                engine_version: env!["CARGO_PKG_VERSION"].to_owned(),
            }))
        }
        None => Ok(None),
    }
}

pub async fn delete_by_bot_id(bot_id: &str, db: &DatabaseConnection) -> Result<()> {
    Bot::delete_many()
        .filter(bot::Column::BotId.eq(bot_id))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn delete_by_id(id: &str, db: &DatabaseConnection) -> Result<()> {
    Bot::delete_by_id(id).exec(db).await?;
    Ok(())
}
