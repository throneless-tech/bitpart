use chrono::NaiveDateTime;
use csml_interpreter::data::Client;
use sea_orm::*;
use sea_query::Expr;
use uuid;

use super::entities::{prelude::*, *};
use crate::error::BitpartError;

pub async fn create(
    flow_id: &str,
    step_id: &str,
    client: &Client,
    expires_at: Option<NaiveDateTime>,
    db: &DatabaseConnection,
) -> Result<String, BitpartError> {
    let id = uuid::Uuid::new_v4().to_string();
    let entry = conversation::ActiveModel {
        id: ActiveValue::Set(id.clone()),
        bot_id: ActiveValue::Set(client.bot_id.to_owned()),
        channel_id: ActiveValue::Set(client.channel_id.to_owned()),
        user_id: ActiveValue::Set(client.user_id.to_owned()),
        flow_id: ActiveValue::Set(flow_id.to_owned()),
        step_id: ActiveValue::Set(step_id.to_owned()),
        expires_at: ActiveValue::Set(expires_at.map(|e| e.to_string())),
        ..Default::default()
    };
    entry.insert(db).await?;
    Ok(id)
}

pub async fn set_status_by_id(
    id: &str,
    status: &str,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let entry = Conversation::find_by_id(id).one(db).await?;
    match entry {
        Some(e) => {
            let mut e: conversation::ActiveModel = e.into();
            e.status = ActiveValue::Set(status.to_owned());
            e.update(db).await?;
            Ok(())
        }
        None => Ok(()),
    }
}

pub async fn set_status_by_client(
    client: &Client,
    status: &str,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    Conversation::update_many()
        .col_expr(conversation::Column::Status, Expr::value(status.to_owned()))
        .filter(conversation::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(conversation::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(conversation::Column::UserId.eq(client.user_id.to_owned()))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn get_latest_by_client(
    client: &Client,
    db: &DatabaseConnection,
) -> Result<Option<conversation::Model>, BitpartError> {
    let entry = Conversation::find()
        .filter(conversation::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(conversation::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(conversation::Column::UserId.eq(client.user_id.to_owned()))
        .order_by(conversation::Column::CreatedAt, Order::Desc)
        .one(db)
        .await?;

    Ok(entry)
}

pub async fn get_by_client(
    client: &Client,
    limit: Option<u64>,
    offset: Option<u64>,
    db: &DatabaseConnection,
) -> Result<Vec<conversation::Model>, BitpartError> {
    let entry = Conversation::find()
        .filter(conversation::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(conversation::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(conversation::Column::UserId.eq(client.user_id.to_owned()))
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?;

    Ok(entry)
}

pub async fn update(
    id: &str,
    flow_id: Option<String>,
    step_id: Option<String>,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    match (flow_id, step_id) {
        (Some(flow_id), Some(step_id)) => {
            if let Some(entry) = Conversation::find_by_id(id).one(db).await? {
                let mut entry: conversation::ActiveModel = entry.into();
                entry.flow_id = ActiveValue::Set(flow_id.to_string());
                entry.step_id = ActiveValue::Set(step_id.to_string());
                entry.update(db).await?;
            }
        }
        (Some(flow_id), _) => {
            if let Some(entry) = Conversation::find_by_id(id).one(db).await? {
                let mut entry: conversation::ActiveModel = entry.into();
                entry.flow_id = ActiveValue::Set(flow_id.to_string());
                entry.update(db).await?;
            }
        }
        (_, Some(step_id)) => {
            if let Some(entry) = Conversation::find_by_id(id).one(db).await? {
                let mut entry: conversation::ActiveModel = entry.into();
                entry.step_id = ActiveValue::Set(step_id.to_string());
                entry.update(db).await?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub async fn delete_by_client(
    client: &Client,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    Conversation::delete_many()
        .filter(conversation::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(conversation::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(conversation::Column::UserId.eq(client.user_id.to_owned()))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn delete_by_bot_id(bot_id: &str, db: &DatabaseConnection) -> Result<(), BitpartError> {
    Conversation::delete_many()
        .filter(conversation::Column::BotId.eq(bot_id.to_owned()))
        .exec(db)
        .await?;
    Ok(())
}
