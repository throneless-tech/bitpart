use chrono::NaiveDateTime;
use csml_interpreter::data::Client;
use sea_orm::*;
use serde_json::Value;
use uuid;

use super::entities::{prelude::*, *};
use crate::data::ConversationData;
use crate::error::BitpartError;

pub async fn create(
    data: &ConversationData,
    messages: &[Value],
    interaction_order: i32,
    direction: &str,
    expires_at: Option<NaiveDateTime>,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
}

pub async fn delete_by_client(
    client: &Client,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
}
pub async fn get_by_client(client: &Client, db: &DatabaseConnection) -> Result<(), BitpartError> {}
