use csml_interpreter::data::CsmlBot;
use sea_orm::*;
use std::env;
use uuid;

use super::entities::{prelude::*, *};
use crate::error::BitpartError;
use crate::server::data::{BotVersion, SerializedCsmlBot};

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
        bot: bot.to_bot(),
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
                bot: bot.to_bot(),
                version_id: bot.id.to_string(),
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
                bot: bot.to_bot(),
                version_id: bot.id.to_string(),
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
    println!("bot_id: {:?}", bot_id);
    let entry = Bot::find()
        .filter(bot::Column::BotId.eq(bot_id))
        .order_by(bot::Column::CreatedAt, Order::Desc)
        .one(db)
        .await?;

    println!("Entry: {:?}", entry);

    match entry {
        Some(e) => {
            let bot: SerializedCsmlBot = serde_json::from_str(&e.bot)?;

            Ok(Some(BotVersion {
                bot: bot.to_bot(),
                version_id: bot.id.to_string(),
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
