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
    if messages.len() == 0 {
        return Ok(());
    }

    let mut new_messages = vec![];

    for (message_order, message) in messages.iter().enumerate() {
        let message = message::ActiveModel {
            id: ActiveValue::Set(uuid::Uuid::new_v4().to_string()),
            conversation_id: ActiveValue::Set(data.conversation_id.to_owned()),
            flow_id: ActiveValue::Set(data.context.flow.to_owned()),
            step_id: ActiveValue::Set(data.context.step.get_step_ref().to_owned()),
            direction: ActiveValue::Set(direction.to_owned()),
            payload: ActiveValue::Set(message.to_string()),
            content_type: ActiveValue::Set(message["content_type"].to_string()),
            message_order: ActiveValue::Set(message_order as i32),
            interaction_order: ActiveValue::Set(interaction_order),
            expires_at: ActiveValue::Set(expires_at.map(|e| e.to_string())),
            ..Default::default()
        };

        new_messages.push(message);
    }

    Message::insert_many(new_messages).exec(db).await?;

    Ok(())
}

pub async fn delete_by_client(
    client: &Client,
    db: &DatabaseConnection,
) -> Result<(), BitpartError> {
    let conversations = super::conversation::get_by_client(client, db).await?;
    for convo in conversations {
        Message::delete_many()
            .filter(message::Column::ConversationId.eq(convo.id.to_owned()))
            .exec(db)
            .await?;
    }
    Ok(())
}

pub async fn get_by_client(
    client: &Client,
    db: &DatabaseConnection,
) -> Result<Vec<message::Model>, BitpartError> {
    let mut messages = vec![];
    let conversations = super::conversation::get_by_client(client, db).await?;
    for convo in conversations {
        let entry = Message::find()
            .filter(message::Column::ConversationId.eq(convo.id.to_owned()))
            .all(db)
            .await?;
        messages.extend(entry);
    }

    Ok(messages)
}
