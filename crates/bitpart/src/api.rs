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

use bitpart_common::{
    csml::Request,
    error::{BitpartErrorKind, Result},
};
use csml_interpreter::{
    data::{CsmlBot, CsmlResult},
    load_components, search_for_modules, validate_bot,
};
use sea_orm::DatabaseConnection;
use tokio::sync::oneshot;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{
    channels::signal,
    csml::conversation,
    csml::data::BotVersion,
    db::{self, entities::channel},
};

#[derive(Clone)]
pub struct ApiState {
    pub db: DatabaseConnection,
    pub auth: String,
    pub token: CancellationToken,
    pub tracker: TaskTracker,
    pub attachments_dir: PathBuf,
    pub manager: Box<signal::SignalManager>,
}

/*
Bot
*/

pub async fn create_bot(mut bot: CsmlBot, state: &ApiState) -> Result<BotVersion> {
    bot.native_components = match load_components() {
        Ok(components) => Some(components),
        Err(err) => return Err(BitpartErrorKind::Interpreter(err.format_error()).into()),
    };

    if let Err(err) = search_for_modules(&mut bot) {
        return Err(BitpartErrorKind::Api(format!("{:?}", err)).into());
    }

    match validate_bot(&bot) {
        CsmlResult {
            errors: Some(errors),
            ..
        } => Err(BitpartErrorKind::Api(format!("{:?}", errors)).into()),
        CsmlResult { .. } => {
            let created = db::bot::create(bot, &state.db).await?;
            Ok(created)
        }
    }
}

pub async fn list_bots(
    limit: Option<u64>,
    offset: Option<u64>,
    state: &ApiState,
) -> Result<Vec<String>> {
    let list = db::bot::list(limit, offset, &state.db).await?;
    Ok(list)
}

pub async fn read_bot(id: &str, state: &ApiState) -> Result<Option<BotVersion>> {
    if let Some(bot) = db::bot::get_latest_by_bot_id(id, &state.db).await? {
        Ok(Some(bot))
    } else {
        Ok(None)
    }
}

pub async fn delete_bot(id: &str, state: &ApiState) -> Result<()> {
    db::bot::delete_by_bot_id(id, &state.db).await?;
    db::channel::delete_by_bot_id(id, &state.db).await?;
    db::memory::delete_by_bot_id(id, &state.db).await
}

pub async fn get_bot_versions(
    id: &str,
    limit: Option<u64>,
    offset: Option<u64>,
    state: &ApiState,
) -> Result<Vec<BotVersion>> {
    db::bot::get(id, limit, offset, &state.db).await
}

pub async fn get_bot_version(id: &str, state: &ApiState) -> Result<Option<BotVersion>> {
    db::bot::get_by_id(id, &state.db).await
}

pub async fn touch_bot_version(
    id: &str,
    version_id: &str,
    state: &ApiState,
) -> Result<Option<BotVersion>> {
    db::bot::touch(id, version_id, &state.db).await
}

pub async fn get_bot_diff(
    version_a: &str,
    version_b: &str,
    state: &ApiState,
) -> Result<(Option<BotVersion>, Option<BotVersion>)> {
    let a = db::bot::get_by_id(version_a, &state.db).await?;
    let b = db::bot::get_by_id(version_b, &state.db).await?;
    Ok((a, b))
}

pub async fn delete_bot_version(id: &str, state: &ApiState) -> Result<()> {
    db::bot::delete_by_id(id, &state.db).await
}

#[cfg(test)]
mod test_bot {
    use crate::utils::get_test_socket;
    use serde_json::json;

    #[tokio::test]
    async fn it_should_create_a_bot() {
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

        socket.assert_receive_text_contains("Hello").await
    }

    #[tokio::test]
    async fn it_should_get_a_bot() {
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

        socket
            .send_json(&json!({
                "message_type": "ReadBot",
                "data": {
                    "id": "bot_id"
                }
            }))
            .await;

        socket.assert_receive_text_contains("Hello").await
    }

    #[tokio::test]
    async fn it_should_delete_a_bot() {
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
                "message_type": "DeleteBot",
                "data": {
                    "id": "bot_id",
                }
            }))
            .await;

        socket
            .assert_receive_json(&json!({
                "message_type": "Response",
                "data": {
                    "response_type": "DeleteBot",
                    "response": serde_json::Value::Null
                }
            }))
            .await;

        socket
            .send_json(&json!({
                "message_type": "ReadBot",
                "data": {
                    "id": "bot_id"
                }
            }))
            .await;

        socket
            .assert_receive_json(&json!({
                "message_type": "Response",
                "data": {
                    "response_type": "ReadBot",
                    "response": serde_json::Value::Null
                }
            }))
            .await
    }

    #[tokio::test]
    async fn it_should_get_multiple_versions() {
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

        socket
            .send_json(&json!({
                "message_type": "ListBots",
            }))
            .await;

        socket.assert_receive_text_contains("Hello").await
    }
}

/*
Request
*/

pub async fn process_request(
    body: &Request,
    db: &DatabaseConnection,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    match conversation::start(body, db).await {
        Ok(res) => Ok(res),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod test_request {
    use crate::utils::get_test_socket;
    use serde_json::{Value, json};

    #[tokio::test]
    async fn it_should_send_request() {
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
                "message_type": "ChatRequest",
                "data": {
                    "bot_id": "bot_id",
                        "event": {
                            "id": "request_id",
                            "client": {
                                "user_id": "user_id",
                                "channel_id": "channel_id",
                                "bot_id": "bot_id"
                            },
                            "payload": {
                              "content_type": "text" ,
                              "content": {
                                "text": "test"
                              }
                            },
                            "metadata": Value::Null,
                }
                }
            }))
            .await;

        socket.assert_receive_text_contains("Hello").await
    }
}

/*
Channels
*/

pub async fn create_channel(id: &str, bot_id: &str, state: &ApiState) -> Result<String> {
    db::channel::create(id, bot_id, &state.db).await
}

pub async fn link_channel(
    id: &str,
    bot_id: &str,
    device_name: &str,
    attachments_dir: PathBuf,
    state: &ApiState,
) -> Result<String> {
    let db_id = db::channel::create(id, bot_id, &state.db).await?;
    let (send, recv) = oneshot::channel();
    let contents = signal::ChannelMessageContents::LinkChannel {
        id: db_id.clone(),
        device_name: device_name.to_owned(),
        attachments_dir,
    };
    let msg = signal::ChannelMessage {
        msg: contents,
        db: state.db.clone(),
        token: state.token.clone(),
        tracker: state.tracker.clone(),
        sender: send,
    };
    state.manager.send(msg);
    Ok(recv.await?)
}

pub async fn start_channel(channel_id: &str, state: &ApiState) -> Result<String> {
    let (send, recv) = oneshot::channel();
    let contents = signal::ChannelMessageContents::StartChannel {
        id: channel_id.to_owned(),
        attachments_dir: state.attachments_dir.clone(),
    };
    let msg = signal::ChannelMessage {
        msg: contents,
        db: state.db.clone(),
        token: state.token.clone(),
        tracker: state.tracker.clone(),
        sender: send,
    };
    state.manager.send(msg);
    Ok(recv.await?)
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
    db::channel::delete(id, bot_id, &state.db).await
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
