// presage-store-bitpart
// Copyright (C) 2025 Throneless Tech
//
// This code is derived in part from code from the Presage project:
// Copyright (C) 2024 Gabriel Féron

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
use uuid;

use crate::error::BitpartStoreError;

pub async fn get(
    channel_id: &str,
    tree: &str,
    key: &str,
    db: &DatabaseConnection,
) -> Result<Option<String>, BitpartStoreError> {
    let existing = ChannelState::find()
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .filter(channel_state::Column::Tree.eq(tree))
        .filter(channel_state::Column::Key.eq(key))
        .one(db)
        .await?;

    if let Some(entry) = existing {
        Ok(Some(entry.value))
    } else {
        Ok(None)
    }
}

pub async fn get_all(
    channel_id: &str,
    tree: &str,
    db: &DatabaseConnection,
) -> Result<Vec<(String, String)>, BitpartStoreError> {
    let existing = ChannelState::find()
        .select_only()
        .column(channel_state::Column::Key)
        .column(channel_state::Column::Value)
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .filter(channel_state::Column::Tree.eq(tree))
        // .order_by_asc(channel_state::Column::Key)
        .into_tuple()
        .all(db)
        .await?;

    Ok(existing)
}

pub async fn get_trees(
    channel_id: &str,
    db: &DatabaseConnection,
) -> Result<Vec<String>, BitpartStoreError> {
    let existing = ChannelState::find()
        .select_only()
        .column(channel_state::Column::Tree)
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .group_by(channel_state::Column::Tree)
        .into_tuple()
        .all(db)
        .await?;

    Ok(existing)
}

pub async fn set<V: Into<String>>(
    channel_id: &str,
    tree: &str,
    key: &str,
    value: V,
    db: &DatabaseConnection,
) -> Result<bool, BitpartStoreError> {
    let existing = ChannelState::find()
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .filter(channel_state::Column::Tree.eq(tree))
        .filter(channel_state::Column::Key.eq(key))
        .one(db)
        .await?;

    let existing = if let Some(entry) = existing {
        let mut to_update: channel_state::ActiveModel = entry.into();
        to_update.value = ActiveValue::Set(value.into());
        to_update.update(db).await?;
        true
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        let to_insert = channel_state::ActiveModel {
            id: ActiveValue::Set(id.clone()),
            channel_id: ActiveValue::Set(channel_id.to_owned()),
            tree: ActiveValue::Set(tree.to_owned()),
            key: ActiveValue::Set(key.to_owned()),
            value: ActiveValue::Set(value.into()),
            ..Default::default()
        };
        to_insert.insert(db).await?;
        false
    };

    Ok(existing)
}

pub async fn remove(
    channel_id: &str,
    tree: &str,
    key: &str,
    db: &DatabaseConnection,
) -> Result<u64, BitpartStoreError> {
    let existing = ChannelState::find()
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .filter(channel_state::Column::Tree.eq(tree))
        .filter(channel_state::Column::Key.eq(key))
        .one(db)
        .await?;

    if let Some(entry) = existing {
        Ok(entry.delete(db).await?.rows_affected)
    } else {
        Ok(0)
    }
}

pub async fn remove_all(
    channel_id: &str,
    tree: &str,
    db: &DatabaseConnection,
) -> Result<u64, BitpartStoreError> {
    let existing = ChannelState::delete_many()
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .filter(channel_state::Column::Tree.eq(tree))
        .exec(db)
        .await?;

    Ok(existing.rows_affected)
}

pub async fn remove_like(
    channel_id: &str,
    tree: &str,
    key: &str,
    db: &DatabaseConnection,
) -> Result<u64, BitpartStoreError> {
    let existing = ChannelState::delete_many()
        .filter(channel_state::Column::ChannelId.eq(channel_id))
        .filter(channel_state::Column::Tree.eq(tree))
        .filter(channel_state::Column::Key.like(key))
        .exec(db)
        .await?;

    Ok(existing.rows_affected)
}
