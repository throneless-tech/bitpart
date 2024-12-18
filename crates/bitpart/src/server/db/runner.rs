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
    let model = runner::ActiveModel {
        id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
        bot_id: ActiveValue::Set(bot_id.to_owned()),
        channel_id: ActiveValue::Set(channel_id.to_owned()),
        ..Default::default()
    };

    model.insert(db).await?;

    Ok(())
}

pub async fn list(
    limit: Option<u64>,
    offset: Option<u64>,
    db: &DatabaseConnection,
) -> Result<Vec<String>, BitpartError> {
    let entries = Runner::find()
        .column(runner::Column::Id)
        .group_by(runner::Column::BotId)
        .order_by(runner::Column::CreatedAt, Order::Desc)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?;

    Ok(entries.into_iter().map(|e| e.id.to_string()).collect())
}

pub async fn get(
    channel_id: &str,
    bot_id: &str,
    db: &DatabaseConnection,
) -> Result<Option<runner::Model>, BitpartError> {
    let entries = Runner::find()
        .filter(runner::Column::BotId.eq(bot_id))
        .filter(runner::Column::ChannelId.eq(channel_id))
        .one(db)
        .await?;

    Ok(entries)
}

pub async fn get_by_id(
    id: &str,
    db: &DatabaseConnection,
) -> Result<Option<runner::Model>, BitpartError> {
    let entries = Runner::find_by_id(id).one(db).await?;

    Ok(entries)
}

pub async fn set(
    bot_id: &str,
    channel_id: &str,
    state: &Value,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let Some(existing) = Runner::find()
        .filter(runner::Column::BotId.eq(bot_id))
        .filter(runner::Column::ChannelId.eq(channel_id))
        .one(db)
        .await?
    else {
        let entry = runner::ActiveModel {
            id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
            bot_id: ActiveValue::Set(bot_id.to_owned()),
            channel_id: ActiveValue::Set(channel_id.to_owned()),
            state: ActiveValue::Set(state.to_string()),
            ..Default::default()
        };
        entry.insert(db).await?;
        return Ok(());
    };

    let mut existing: runner::ActiveModel = existing.into();
    existing.state = ActiveValue::Set(state.to_string());
    existing.update(db).await?;
    Ok(())
}

pub async fn delete(
    bot_id: &str,
    channel_id: &str,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let entry = Runner::find()
        .filter(runner::Column::BotId.eq(bot_id.to_owned()))
        .filter(runner::Column::ChannelId.eq(channel_id.to_owned()))
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
    Runner::delete_by_id(id).exec(db).await?;
    Ok(())
}
