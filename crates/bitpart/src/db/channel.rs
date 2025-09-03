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

use bitpart_common::error::{BitpartErrorKind, Result};
use sea_orm::*;
use uuid;

use super::entities::{prelude::*, *};

pub async fn create(channel_id: &str, bot_id: &str, db: &DatabaseConnection) -> Result<String> {
    let Some(existing) = Channel::find()
        .filter(channel::Column::BotId.eq(bot_id))
        .filter(channel::Column::ChannelId.eq(channel_id))
        .one(db)
        .await?
    else {
        let id = uuid::Uuid::new_v4().to_string();
        let entry = channel::ActiveModel {
            id: ActiveValue::Set(id.clone()),
            bot_id: ActiveValue::Set(bot_id.to_owned()),
            channel_id: ActiveValue::Set(channel_id.to_owned()),
            ..Default::default()
        };
        entry.insert(db).await?;
        return Ok(id);
    };
    Ok(existing.id)
}

pub async fn list(
    limit: Option<u64>,
    offset: Option<u64>,
    db: &DatabaseConnection,
) -> Result<Vec<channel::Model>> {
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
) -> Result<Option<channel::Model>> {
    let entries = Channel::find()
        .filter(channel::Column::BotId.eq(bot_id))
        .filter(channel::Column::ChannelId.eq(channel_id))
        .one(db)
        .await?;

    Ok(entries)
}

pub async fn get_by_id(id: &str, db: &DatabaseConnection) -> Result<Option<channel::Model>> {
    let entries = Channel::find_by_id(id).one(db).await?;

    Ok(entries)
}

pub async fn get_by_bot_id(bot_id: &str, db: &DatabaseConnection) -> Result<Vec<channel::Model>> {
    let entries = Channel::find()
        .filter(channel::Column::BotId.eq(bot_id.to_owned()))
        .all(db)
        .await?;

    Ok(entries)
}

pub async fn delete(channel_id: &str, bot_id: &str, db: &DatabaseConnection) -> Result<()> {
    let entry = Channel::find()
        .filter(channel::Column::BotId.eq(bot_id.to_owned()))
        .filter(channel::Column::ChannelId.eq(channel_id.to_owned()))
        .one(db)
        .await?;

    if let Some(e) = entry {
        e.delete(db).await?;
        Ok(())
    } else {
        Err(BitpartErrorKind::Db(DbErr::RecordNotFound(bot_id.to_owned())).into())
    }
}

pub async fn delete_by_bot_id(bot_id: &str, db: &DatabaseConnection) -> Result<()> {
    let entry = Channel::find()
        .filter(channel::Column::BotId.eq(bot_id.to_owned()))
        .one(db)
        .await?;

    if let Some(e) = entry {
        e.delete(db).await?;
    }

    Ok(())
}

pub async fn delete_by_id(id: &str, db: &DatabaseConnection) -> Result<()> {
    Channel::delete_by_id(id).exec(db).await?;
    Ok(())
}
