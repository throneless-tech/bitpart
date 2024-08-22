use chrono::NaiveDateTime;
use csml_interpreter::data::Client;
use sea_orm::*;
use serde_json::Value;
use uuid;

use super::entities::{prelude::*, *};
use crate::error::BitpartError;

pub async fn create(
    client: &Client,
    key: &str,
    value: Value,
    expires_at: Option<NaiveDateTime>,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let entry = memory::ActiveModel {
        id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
        bot_id: ActiveValue::Set(client.bot_id.to_owned()),
        channel_id: ActiveValue::Set(client.channel_id.to_owned()),
        user_id: ActiveValue::Set(client.user_id.to_owned()),
        key: ActiveValue::Set(key.to_owned()),
        value: ActiveValue::Set(value.to_string()),
        expires_at: ActiveValue::Set(expires_at.map(|e| e.to_string())),
        ..Default::default()
    };
    entry.insert(db).await?;
    Ok(())
}

pub async fn get(
    client: &Client,
    key: &str,
    db: &DatabaseConnection,
) -> Result<Option<memory::Model>, BitpartError> {
    let entry = Memory::find()
        .filter(memory::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(memory::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(memory::Column::UserId.eq(client.user_id.to_owned()))
        .filter(memory::Column::Key.eq(key))
        .one(db)
        .await?;

    Ok(entry)
}

pub async fn get_by_client(
    client: &Client,
    db: &DatabaseConnection,
) -> Result<Vec<memory::Model>, BitpartError> {
    let entry = Memory::find()
        .filter(memory::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(memory::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(memory::Column::UserId.eq(client.user_id.to_owned()))
        .all(db)
        .await?;

    Ok(entry)
}

pub async fn delete(
    client: &Client,
    key: &str,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let entry = Memory::find()
        .filter(memory::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(memory::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(memory::Column::UserId.eq(client.user_id.to_owned()))
        .filter(memory::Column::Key.eq(key))
        .one(db)
        .await?;

    match entry {
        Some(e) => {
            e.delete(db).await?;
        }
        None => {}
    }

    Ok(())
}

pub async fn delete_by_client(
    client: &Client,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    Memory::delete_many()
        .filter(memory::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(memory::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(memory::Column::UserId.eq(client.user_id.to_owned()))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn delete_by_bot_id(bot_id: &str, db: &DatabaseConnection) -> Result<(), BitpartError> {
    Memory::delete_many()
        .filter(memory::Column::BotId.eq(bot_id.to_owned()))
        .exec(db)
        .await?;
    Ok(())
}
