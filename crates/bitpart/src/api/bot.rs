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
use csml_interpreter::{
    data::{CsmlBot, CsmlResult},
    load_components, search_for_modules, validate_bot,
};

use crate::{api::ApiState, csml::data::BotVersion, db};

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
            let created = db::bot::create(bot, &state.pool).await?;
            Ok(created)
        }
    }
}

pub async fn list_bots(
    limit: Option<u64>,
    offset: Option<u64>,
    state: &ApiState,
) -> Result<Vec<String>> {
    let list = db::bot::list(limit, offset, &state.pool).await?;
    Ok(list)
}

pub async fn read_bot(id: &str, state: &ApiState) -> Result<Option<BotVersion>> {
    if let Some(bot) = db::bot::get_latest_by_bot_id(id, &state.pool).await? {
        Ok(Some(bot))
    } else {
        Ok(None)
    }
}

pub async fn delete_bot(id: &str, state: &ApiState) -> Result<()> {
    db::bot::delete_by_bot_id(id, &state.pool).await?;
    db::memory::delete_by_bot_id(id, &state.pool).await?;
    let channels = db::channel::get_by_bot_id(id, &state.pool).await?;
    for channel in channels.iter() {
        crate::api::channel::delete_channel(&channel.channel_id, id, state).await?;
    }
    Ok(())
}

pub async fn get_bot_versions(
    id: &str,
    limit: Option<u64>,
    offset: Option<u64>,
    state: &ApiState,
) -> Result<Vec<BotVersion>> {
    db::bot::get(id, limit, offset, &state.pool).await
}

pub async fn get_bot_version(id: &str, state: &ApiState) -> Result<Option<BotVersion>> {
    db::bot::get_by_id(id, &state.pool).await
}

pub async fn touch_bot_version(
    id: &str,
    version_id: &str,
    state: &ApiState,
) -> Result<Option<BotVersion>> {
    db::bot::touch(id, version_id, &state.pool).await
}

pub async fn get_bot_diff(
    version_a: &str,
    version_b: &str,
    state: &ApiState,
) -> Result<(Option<BotVersion>, Option<BotVersion>)> {
    let a = db::bot::get_by_id(version_a, &state.pool).await?;
    let b = db::bot::get_by_id(version_b, &state.pool).await?;
    Ok((a, b))
}

pub async fn delete_bot_version(id: &str, state: &ApiState) -> Result<()> {
    db::bot::delete_by_id(id, &state.pool).await
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
