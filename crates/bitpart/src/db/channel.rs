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

    if let Some(e) = entry {
        e.delete(db).await?;
    }

    Ok(())
}

pub async fn delete_by_id(id: &str, db: &DatabaseConnection) -> Result<(), BitpartError> {
    Channel::delete_by_id(id).exec(db).await?;
    Ok(())
}
