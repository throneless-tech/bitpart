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

use std::path::PathBuf;

use bitpart_common::error::{BitpartErrorKind, Result};
use tokio::sync::oneshot;

use crate::{
    api::ApiState,
    channels::signal,
    db,
    db::entities::channel,
};

pub async fn create_channel(id: &str, bot_id: &str, state: &ApiState) -> Result<String> {
    db::channel::create(id, bot_id, &state.db).await
}

pub async fn link_channel(
    id: &str,
    bot_id: &str,
    device_name: &str,
    attachments_dir: PathBuf,
    state: &mut ApiState,
) -> Result<String> {
    let db_id = db::channel::create(id, bot_id, &state.db).await?;
    let (send, recv) = oneshot::channel();
    let contents = signal::ChannelMessageContents::LinkChannel {
        id: db_id.clone(),
        device_name: device_name.to_owned(),
        attachments_dir,
    };
    let token = state.parent_token.child_token();
    let msg_token = token.clone();
    let mut data = state.tokens.lock().await;
    data.insert((bot_id.to_owned(), id.to_owned()), token);
    let msg = signal::ChannelMessage {
        msg: contents,
        db: state.db.clone(),
        token: msg_token,
        tracker: state.tracker.clone(),
        sender: send,
    };
    state.manager.send(msg).await?;
    Ok(recv.await?)
}

pub async fn start_channel(channel_id: &str, bot_id: &str, state: &mut ApiState) -> Result<String> {
    let (send, recv) = oneshot::channel();
    let contents = signal::ChannelMessageContents::StartChannel {
        id: channel_id.to_owned(),
        attachments_dir: state.attachments_dir.clone(),
    };
    let mut data = state.tokens.lock().await;
    let token = data
        .entry((bot_id.to_owned(), channel_id.to_owned()))
        .or_insert(state.parent_token.child_token());
    let msg = signal::ChannelMessage {
        msg: contents,
        db: state.db.clone(),
        token: token.clone(),
        tracker: state.tracker.clone(),
        sender: send,
    };
    state.manager.send(msg).await?;
    Ok(recv.await?)
}

pub async fn reset_channel(channel_id: &str, bot_id: &str, state: &mut ApiState) -> Result<String> {
    if let Some(channel) = db::channel::get(channel_id, bot_id, &state.db).await? {
        let (send, recv) = oneshot::channel();
        let contents = signal::ChannelMessageContents::ResetSessions {
            id: channel.id.to_owned(),
        };
        let mut data = state.tokens.lock().await;
        let token = data
            .entry((bot_id.to_owned(), channel_id.to_owned()))
            .or_insert(state.parent_token.child_token());
        let msg = signal::ChannelMessage {
            msg: contents,
            db: state.db.clone(),
            token: token.clone(),
            tracker: state.tracker.clone(),
            sender: send,
        };
        state.manager.send(msg).await?;
        Ok(recv.await?)
    } else {
        Err(BitpartErrorKind::Api("Resetting non-existent channel".into()).into())
    }
}

pub async fn read_channel(
    id: &str,
    bot_id: &str,
    state: &ApiState,
) -> Result<Option<channel::Model>> {
    let channel = db::channel::get(id, bot_id, &state.db).await?;
    Ok(channel)
}

pub async fn list_channels(
    limit: Option<u64>,
    offset: Option<u64>,
    state: &ApiState,
) -> Result<Option<Vec<channel::Model>>> {
    match db::channel::list(limit, offset, &state.db).await {
        Ok(v) if !v.is_empty() => Ok(Some(v)),
        _ => Ok(None),
    }
}

pub async fn delete_channel(id: &str, bot_id: &str, state: &ApiState) -> Result<()> {
    db::channel::delete(id, bot_id, &state.db).await?;
    let data = state.tokens.lock().await;
    if let Some(token) = data.get(&(bot_id.to_owned(), id.to_owned())) {
        token.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod test_channel {
    use crate::utils::get_test_socket;
    use serde_json::json;

    #[tokio::test]
    async fn it_should_create_a_channel() {
        let mut socket = get_test_socket().await;

        socket
            .send_json(&json!({
                "message_type": "CreateBot",
                "data": {
                    "id": "bot_id",
                    "name": "test",
                    "flows": [
                      {
                        "id": "Default",
                        "name": "Default",
                        "content": "start: say \"Hello\" goto end",
                        "commands": [],
                      }
                    ],
                    "default_flow": "Default",
                }
            }))
            .await;

        socket.assert_receive_text_contains("Hello").await;

        socket
            .send_json(&json!({
                "message_type": "CreateChannel",
                "data": {
                    "id": "test",
                    "bot_id": "bot_id",
                }
            }))
            .await;

        socket.assert_receive_text_contains("CreateChannel").await;
    }

    #[tokio::test]
    async fn it_should_get_a_channel() {
        let mut socket = get_test_socket().await;

        socket
            .send_json(&json!({
                "message_type": "CreateBot",
                "data": {
                    "id": "bot_id",
                    "name": "test",
                    "flows": [
                      {
                        "id": "Default",
                        "name": "Default",
                        "content": "start: say \"Hello\" goto end",
                        "commands": [],
                      }
                    ],
                    "default_flow": "Default",
                }
            }))
            .await;

        socket.assert_receive_text_contains("Hello").await;

        socket
            .send_json(&json!({
                "message_type": "CreateChannel",
                "data": {
                    "id": "test",
                    "bot_id": "bot_id",
                }
            }))
            .await;

        socket.assert_receive_text_contains("CreateChannel").await;

        socket
            .send_json(&json!({
                "message_type": "ReadChannel",
                "data": {
                    "id": "test",
                    "bot_id": "bot_id",
                }
            }))
            .await;

        socket.assert_receive_text_contains("test").await
    }

    #[tokio::test]
    async fn it_should_delete_a_channel() {
        let mut socket = get_test_socket().await;

        socket
            .send_json(&json!({
                "message_type": "CreateBot",
                "data": {
                    "id": "bot_id",
                    "name": "test",
                    "flows": [
                      {
                        "id": "Default",
                        "name": "Default",
                        "content": "start: say \"Hello\" goto end",
                        "commands": [],
                      }
                    ],
                    "default_flow": "Default",
                }
            }))
            .await;

        socket.assert_receive_text_contains("Hello").await;

        socket
            .send_json(&json!({
                "message_type": "CreateChannel",
                "data": {
                    "id": "test",
                    "bot_id": "bot_id",
                }
            }))
            .await;

        socket.assert_receive_text_contains("CreateChannel").await;

        socket
            .send_json(&json!({
                "message_type": "DeleteChannel",
                "data": {
                    "id": "test",
                    "bot_id": "bot_id",
                }
            }))
            .await;

        socket
            .assert_receive_json(&json!({
                "message_type": "Response",
                "data": {
                    "response_type": "DeleteChannel",
                    "response": serde_json::Value::Null
                }
            }))
            .await;

        socket
            .send_json(&json!({
                "message_type": "ReadChannel",
                "data": {
                    "id": "test",
                    "bot_id": "bot_id"
                }
            }))
            .await;

        socket
            .assert_receive_json(&json!({
                "message_type": "Response",
                "data": {
                    "response_type": "ReadChannel",
                    "response": serde_json::Value::Null
                }
            }))
            .await
    }

    #[tokio::test]
    async fn it_should_get_multiple_channels() {
        let mut socket = get_test_socket().await;

        socket
            .send_json(&json!({
                "message_type": "CreateBot",
                "data": {
                    "id": "bot_id",
                    "name": "test",
                    "flows": [
                      {
                        "id": "Default",
                        "name": "Default",
                        "content": "start: say \"Hello\" goto end",
                        "commands": [],
                      }
                    ],
                    "default_flow": "Default",
                }
            }))
            .await;

        socket.assert_receive_text_contains("Hello").await;

        socket
            .send_json(&json!({
                "message_type": "CreateChannel",
                "data": {
                    "id": "test",
                    "bot_id": "bot_id",
                }
            }))
            .await;

        socket.assert_receive_text_contains("CreateChannel").await;

        socket
            .send_json(&json!({
                "message_type": "CreateChannel",
                "data": {
                    "id": "test2",
                    "bot_id": "bot_id",
                }
            }))
            .await;

        socket.assert_receive_text_contains("CreateChannel").await;

        socket
            .send_json(&json!({
                "message_type": "ListChannels",
            }))
            .await;

        socket.assert_receive_text_contains("test2").await
    }
}
