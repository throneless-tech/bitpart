use super::entities::{prelude::*, *};
use sea_orm::*;

use crate::error::BitpartStoreError;

pub async fn get_by_id(
    id: &str,
    db: &DatabaseConnection,
) -> Result<channel::Model, BitpartStoreError> {
    let entry = Channel::find_by_id(id)
        .one(db)
        .await?
        .ok_or(BitpartStoreError::Store(
            "Failed to find channel by ID".into(),
        ))?;

    Ok(entry)
}
pub async fn set_by_id(
    id: &str,
    state: &str,
    db: &DatabaseConnection,
) -> Result<(), BitpartStoreError> {
    let existing = Channel::find_by_id(id)
        .one(db)
        .await?
        .ok_or(BitpartStoreError::Store(
            "Failed to find channel by ID".into(),
        ))?;
    let mut existing: channel::ActiveModel = existing.into();
    existing.state = ActiveValue::Set(state.to_string());
    existing.update(db).await?;
    Ok(())
}
