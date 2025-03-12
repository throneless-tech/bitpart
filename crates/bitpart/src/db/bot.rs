use csml_interpreter::data::{CsmlBot, CsmlFlow, Module};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use std::env;
use uuid;

use super::entities::{prelude::*, *};
use crate::csml::data::BotVersion;
use crate::error::BitpartError;

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
}

impl Into<CsmlBot> for SerializedCsmlBot {
    fn into(self) -> CsmlBot {
        CsmlBot {
            id: self.id.to_owned(),
            name: self.name.to_owned(),
            apps_endpoint: None,
            flows: self.flows.to_owned(),
            native_components: {
                match self.native_components.to_owned() {
                    Some(value) => match serde_json::from_str(&value) {
                        Ok(serde_json::Value::Object(map)) => Some(map),
                        _ => unreachable!(),
                    },
                    None => None,
                }
            },
            custom_components: {
                match self.custom_components.to_owned() {
                    Some(value) => match serde_json::from_str(&value) {
                        Ok(value) => Some(value),
                        Err(_e) => unreachable!(),
                    },
                    None => None,
                }
            },
            default_flow: self.default_flow.to_owned(),
            bot_ast: None,
            no_interruption_delay: self.no_interruption_delay,
            env: self.env.as_ref().map(|e| serde_json::from_str(&e).unwrap()),
            modules: self.modules.to_owned(),
            multibot: None,
        }
    }
}

pub async fn create(bot: CsmlBot, db: &DatabaseConnection) -> Result<BotVersion, BitpartError> {
    let model = bot::ActiveModel {
        id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
        bot_id: ActiveValue::Set(bot.id.to_owned()),
        bot: ActiveValue::Set(bot.to_json().to_string()),
        engine_version: ActiveValue::Set(env!["CARGO_PKG_VERSION"].to_owned()),
        ..Default::default()
    };

    let entry = model.insert(db).await?;

    let bot: SerializedCsmlBot = serde_json::from_str(&entry.bot).unwrap();

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
) -> Result<Vec<String>, BitpartError> {
    let entries = Bot::find()
        .column(bot::Column::Id)
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
) -> Result<Vec<BotVersion>, BitpartError> {
    let entries = Bot::find()
        .filter(bot::Column::BotId.eq(bot_id))
        .order_by(bot::Column::CreatedAt, Order::Desc)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?;

    Ok(entries
        .into_iter()
        .map(|e| {
            let bot: SerializedCsmlBot = serde_json::from_str(&e.bot).unwrap();
            BotVersion {
                version_id: bot.id.to_string(),
                bot: bot.into(),
                engine_version: env!["CARGO_PKG_VERSION"].to_owned(),
            }
        })
        .collect())
}

pub async fn get_by_id(
    id: &str,
    db: &DatabaseConnection,
) -> Result<Option<BotVersion>, BitpartError> {
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

pub async fn get_latest_by_bot_id(
    bot_id: &str,
    db: &DatabaseConnection,
) -> Result<Option<BotVersion>, BitpartError> {
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

pub async fn delete_by_bot_id(bot_id: &str, db: &DatabaseConnection) -> Result<(), BitpartError> {
    Bot::delete_many()
        .filter(bot::Column::BotId.eq(bot_id))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn delete_by_id(id: &str, db: &DatabaseConnection) -> Result<(), BitpartError> {
    Bot::delete_by_id(id).exec(db).await?;
    Ok(())
}
