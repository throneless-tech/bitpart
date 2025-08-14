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
use chrono::NaiveDateTime;
use csml_interpreter::data::Client;
use sea_orm::*;
use serde_json::Value;

use super::entities::{prelude::*, *};

pub async fn get(
    client: &Client,
    r#type: &str,
    key: &str,
    db: &DatabaseConnection,
) -> Result<Value> {
    let Some(entry) = State::find()
        .filter(state::Column::BotId.eq(&client.bot_id))
        .filter(state::Column::ChannelId.eq(&client.channel_id))
        .filter(state::Column::UserId.eq(&client.user_id))
        .filter(state::Column::Type.eq(r#type))
        .filter(state::Column::Key.eq(key))
        .one(db)
        .await?
    else {
        return Err(BitpartErrorKind::Interpreter("No state found".to_owned()).into());
    };
    Ok(serde_json::from_str(&entry.value)?)
}

pub async fn get_by_client(client: &Client, db: &DatabaseConnection) -> Result<Vec<Value>> {
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
) -> Result<()> {
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
            key: ActiveValue::Set(key.to_owned()),
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
) -> Result<()> {
    let entry = State::find()
        .filter(state::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(state::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(state::Column::UserId.eq(client.user_id.to_owned()))
        .filter(state::Column::Type.eq(r#type))
        .filter(state::Column::Key.eq(key))
        .one(db)
        .await?;

    if let Some(e) = entry {
        e.delete(db).await?;
    }

    Ok(())
}

pub async fn delete_by_client(client: &Client, db: &DatabaseConnection) -> Result<()> {
    State::delete_many()
        .filter(state::Column::BotId.eq(client.bot_id.to_owned()))
        .filter(state::Column::ChannelId.eq(client.channel_id.to_owned()))
        .filter(state::Column::UserId.eq(client.user_id.to_owned()))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn delete_by_bot_id(bot_id: &str, db: &DatabaseConnection) -> Result<()> {
    State::delete_many()
        .filter(state::Column::BotId.eq(bot_id))
        .exec(db)
        .await?;
    Ok(())
}
