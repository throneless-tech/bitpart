use chrono::NaiveDateTime;
use csml_interpreter::data::Client;
use sea_orm::*;
use serde_json::Value;

use super::entities::{prelude::*, *};
use crate::error::BitpartError;

pub async fn get(
    client: &Client,
    r#type: &str,
    key: &str,
    db: &DatabaseConnection,
) -> Result<Value, BitpartError> {
    let Some(entry) = State::find()
        .filter(state::Column::BotId.eq(&client.bot_id))
        .filter(state::Column::ChannelId.eq(&client.channel_id))
        .filter(state::Column::UserId.eq(&client.user_id))
        .filter(state::Column::Type.eq(r#type))
        .filter(state::Column::Key.eq(key))
        .one(db)
        .await?
    else {
        return Err(BitpartError::Interpreter("No state found".to_owned()));
    };
    Ok(entry.value.into())
}

pub async fn get_by_client(
    client: &Client,
    db: &DatabaseConnection,
) -> Result<Vec<Value>, BitpartError> {
    let entries = State::find()
        .filter(state::Column::BotId.eq(&client.bot_id))
        .filter(state::Column::ChannelId.eq(&client.channel_id))
        .filter(state::Column::UserId.eq(&client.user_id))
        .all(db)
        .await?;
    Ok(entries.into_iter().map(|e| e.value.into()).collect())
}

pub async fn set(
    client: &Client,
    r#type: &str,
    key: &str,
    value: &Value,
    expires_at: Option<NaiveDateTime>,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let Some(existing) = State::find()
        .filter(state::Column::BotId.eq(&client.bot_id))
        .filter(state::Column::ChannelId.eq(&client.channel_id))
        .filter(state::Column::UserId.eq(&client.user_id))
        .filter(state::Column::Type.eq(r#type))
        .filter(state::Column::Key.eq(key))
        .one(db)
        .await?
    else {
        let entry = state::ActiveModel {
            id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
            bot_id: ActiveValue::Set(client.bot_id.to_owned()),
            channel_id: ActiveValue::Set(client.channel_id.to_owned()),
            user_id: ActiveValue::Set(client.user_id.to_owned()),
            r#type: ActiveValue::Set(r#type.to_owned().to_owned()),
            value: ActiveValue::Set(value.to_string()),
            expires_at: ActiveValue::Set(expires_at.map(|e| e.to_string())),
            ..Default::default()
        };
        entry.insert(db).await?;
        return Ok(());
    };

    let mut existing: state::ActiveModel = existing.into();
    existing.value = ActiveValue::Set(value.to_string());
    existing.expires_at = ActiveValue::Set(expires_at.map(|e| e.to_string()));
    existing.update(db).await?;
    Ok(())
}

pub async fn delete(
    client: &Client,
    r#type: &str,
    key: &str,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let entry = State::find()
        .filter(state::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(state::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(state::Column::UserId.eq(client.user_id.to_owned()))
        .filter(state::Column::Type.eq(r#type))
        .filter(state::Column::Key.eq(key))
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

// pub async fn set_many(
//     client: &Client,
//     r#type: &str,
//     pairs: Vec<(&str, &Value)>,
//     expires_at: Option<NaiveDateTime>,
//     db: &DatabaseConnection,
// ) -> Result<(), BitpartError> {
// }

pub async fn delete_by_client(
    client: &Client,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    State::delete_many()
        .filter(state::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(state::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(state::Column::UserId.eq(client.user_id.to_owned()))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn delete_by_bot_id(bot_id: &str, db: &DatabaseConnection) -> Result<(), BitpartError> {
    State::delete_many()
        .filter(state::Column::BotId.eq(bot_id))
        .exec(db)
        .await?;
    Ok(())
}
