use super::entities::{prelude::*, *};
use sea_orm::*;
use serde_json::Value;
use uuid;

use crate::error::BitpartError;

pub async fn create(
    channel_id: &str,
    bot_id: &str,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let model = channel::ActiveModel {
        id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
        bot_id: ActiveValue::Set(bot_id.to_owned()),
        channel_id: ActiveValue::Set(channel_id.to_owned()),
        state: ActiveValue::Set("".to_owned()),
        ..Default::default()
    };

    model.insert(db).await?;

    Ok(())
}

pub async fn list(
    limit: Option<u64>,
    offset: Option<u64>,
    db: &DatabaseConnection,
) -> Result<Vec<channel::Model>, BitpartError> {
    let entries = Channel::find()
        .order_by(channel::Column::CreatedAt, Order::Desc)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?;

    Ok(entries)
}

pub async fn get(
    channel_id: &str,
    bot_id: &str,
    db: &DatabaseConnection,
) -> Result<Option<channel::Model>, BitpartError> {
    let entries = Channel::find()
        .filter(channel::Column::BotId.eq(bot_id))
        .filter(channel::Column::ChannelId.eq(channel_id))
        .one(db)
        .await?;

    Ok(entries)
}

pub async fn get_by_id(
    id: &str,
    db: &DatabaseConnection,
) -> Result<Option<channel::Model>, BitpartError> {
    let entries = Channel::find_by_id(id).one(db).await?;

    Ok(entries)
}

pub async fn set(
    bot_id: &str,
    channel_id: &str,
    state: &Value,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let Some(existing) = Channel::find()
        .filter(channel::Column::BotId.eq(bot_id))
        .filter(channel::Column::ChannelId.eq(channel_id))
        .one(db)
        .await?
    else {
        let entry = channel::ActiveModel {
            id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
            bot_id: ActiveValue::Set(bot_id.to_owned()),
            channel_id: ActiveValue::Set(channel_id.to_owned()),
            state: ActiveValue::Set(state.to_string()),
            ..Default::default()
        };
        entry.insert(db).await?;
        return Ok(());
    };

    let mut existing: channel::ActiveModel = existing.into();
    existing.state = ActiveValue::Set(state.to_string());
    existing.update(db).await?;
    Ok(())
}

pub async fn delete(
    channel_id: &str,
    bot_id: &str,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let entry = Channel::find()
        .filter(channel::Column::BotId.eq(bot_id.to_owned()))
        .filter(channel::Column::ChannelId.eq(channel_id.to_owned()))
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

pub async fn delete_by_id(id: &str, db: &DatabaseConnection) -> Result<(), BitpartError> {
    Channel::delete_by_id(id).exec(db).await?;
    Ok(())
}
