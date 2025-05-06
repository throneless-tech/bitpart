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
use chrono::NaiveDateTime;
use csml_interpreter::data::{Client, Memory as CsmlMemory};
use sea_orm::*;
use std::collections::HashMap;
use uuid;

use super::entities::{prelude::*, *};

pub async fn create(
    client: &Client,
    key: &str,
    value: &str,
    expires_at: Option<NaiveDateTime>,
    db: &DatabaseConnection,
) -> Result<()> {
    let entry = memory::ActiveModel {
        id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
        bot_id: ActiveValue::Set(client.bot_id.to_owned()),
        channel_id: ActiveValue::Set(client.channel_id.to_owned()),
        user_id: ActiveValue::Set(client.user_id.to_owned()),
        key: ActiveValue::Set(key.to_owned()),
        value: ActiveValue::Set(value.to_owned()),
        expires_at: ActiveValue::Set(expires_at.map(|e| e.to_string())),
        ..Default::default()
    };
    Memory::insert(entry).exec(db).await?;
    Ok(())
}

pub async fn create_many(
    client: &Client,
    memories: &HashMap<String, CsmlMemory>,
    expires_at: Option<NaiveDateTime>,
    db: &DatabaseConnection,
) -> Result<()> {
    let mut new_memories = vec![];

    for (key, value) in memories.iter() {
        let entry = memory::ActiveModel {
            id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
            bot_id: ActiveValue::Set(client.bot_id.to_owned()),
            channel_id: ActiveValue::Set(client.channel_id.to_owned()),
            user_id: ActiveValue::Set(client.user_id.to_owned()),
            key: ActiveValue::Set(key.to_owned()),
            value: ActiveValue::Set(
                value
                    .value
                    .to_string()
                    .trim_matches(|c| c == '\"' || c == '\'')
                    .to_string(),
            ),
            expires_at: ActiveValue::Set(expires_at.map(|e| e.to_string())),
            ..Default::default()
        };
        new_memories.push(entry);
    }
    if !new_memories.is_empty() {
        Memory::insert_many(new_memories).exec(db).await?;
    }
    Ok(())
}

pub async fn get(
    client: &Client,
    key: &str,
    db: &DatabaseConnection,
) -> Result<Option<memory::Model>> {
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
    limit: Option<u64>,
    offset: Option<u64>,
    db: &DatabaseConnection,
) -> Result<Vec<memory::Model>> {
    let entry = Memory::find()
        .filter(memory::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(memory::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(memory::Column::UserId.eq(client.user_id.to_owned()))
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?;

    Ok(entry)
}

pub async fn get_by_memory(key: &str, db: &DatabaseConnection) -> Result<Vec<memory::Model>> {
    let entry = Memory::find()
        .filter(memory::Column::Key.eq(key))
        .all(db)
        .await?;

    Ok(entry)
}

pub async fn delete(client: &Client, key: &str, db: &DatabaseConnection) -> Result<()> {
    let entry = Memory::find()
        .filter(memory::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(memory::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(memory::Column::UserId.eq(client.user_id.to_owned()))
        .filter(memory::Column::Key.eq(key))
        .one(db)
        .await?;

    if let Some(e) = entry {
        e.delete(db).await?;
    }

    Ok(())
}

pub async fn delete_by_client(client: &Client, db: &DatabaseConnection) -> Result<()> {
    Memory::delete_many()
        .filter(memory::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(memory::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(memory::Column::UserId.eq(client.user_id.to_owned()))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn delete_by_bot_id(bot_id: &str, db: &DatabaseConnection) -> Result<()> {
    Memory::delete_many()
        .filter(memory::Column::BotId.eq(bot_id.to_owned()))
        .exec(db)
        .await?;
    Ok(())
}
