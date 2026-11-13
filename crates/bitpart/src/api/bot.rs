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
use bitpart_csml::{
    data::{CsmlBot, CsmlResult},
    load_components, search_for_modules, validate_bot,
};

use crate::{api::ApiState, csml::data::BotVersion, db, metrics::ChannelStatus};
use serde::Serialize;
use tracing::warn;

#[derive(Debug, Serialize)]
pub struct BotDescription {
    #[serde(flatten)]
    pub bot: BotVersion,
    pub channels: Vec<ChannelStatus>,
}

pub async fn create_bot(
    mut bot: CsmlBot,
    expire_timer: Option<u32>,
    state: &ApiState,
) -> Result<BotVersion> {
    bot.native_components = match load_components() {
        Ok(components) => Some(components),
        Err(err) => return Err(BitpartErrorKind::Interpreter(err.format_error()).into()),
    };

    if let Err(err) = search_for_modules(&mut bot).await {
        return Err(BitpartErrorKind::Api(format!("{:?}", err)).into());
    }

    match validate_bot(&bot) {
        CsmlResult {
            errors: Some(errors),
            ..
        } => Err(BitpartErrorKind::Api(format!("{:?}", errors)).into()),
        CsmlResult { .. } => {
            let created = db::bot::create(bot, expire_timer, &state.pool).await?;
            refresh_bot_count(state).await;
            Ok(created)
        }
    }
}

pub async fn refresh_bot_count(state: &ApiState) {
    match db::bot::count(&state.pool).await {
        Ok(count) => state.metrics.set_bots(count),
        Err(error) => warn!(%error, "could not count bots"),
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

pub async fn read_bot(id: &str, state: &ApiState) -> Result<Option<BotDescription>> {
    if let Some(bot) = db::bot::get_latest_by_bot_id(id, &state.pool).await? {
        let channels: Vec<(String, String)> = db::channel::get_by_bot_id(id, &state.pool)
            .await?
            .into_iter()
            .map(|c| (c.id, c.channel_id))
            .collect();
        Ok(Some(BotDescription {
            bot,
            channels: state.metrics.for_channels(&channels),
        }))
    } else {
        Ok(None)
    }
}

pub async fn delete_bot(id: &str, state: &ApiState) -> Result<()> {
    db::bot::delete_by_bot_id(id, &state.pool).await?;
    refresh_bot_count(state).await;
    db::memory::delete_by_bot_id(id, &state.pool).await?;
    db::message::delete_by_bot_id(id, &state.pool).await?;
    db::conversation::delete_by_bot_id(id, &state.pool).await?;
    db::state::delete_by_bot_id(id, &state.pool).await?;
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
    db::bot::delete_by_id(id, &state.pool).await?;
    refresh_bot_count(state).await;
    Ok(())
}

#[cfg(test)]
mod test_bot {
    use crate::utils::get_test_socket;
    use serde_json::json;

    #[tokio::test]
    async fn it_should_create_a_bot() {
        let (mut socket, _dir) = get_test_socket().await;

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
    async fn it_should_create_a_bot_with_an_expire_timer() {
        let (mut socket, _dir) = get_test_socket().await;

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
                    "expire_timer": 604800,
                }
            }))
            .await;

        socket
            .assert_receive_text_contains("\"expire_timer\":604800")
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
            .assert_receive_text_contains("\"expire_timer\":604800")
            .await
    }

    #[tokio::test]
    async fn it_should_inherit_or_remove_the_expire_timer() {
        let (mut socket, _dir) = get_test_socket().await;

        let bot = |expire_timer: Option<u32>| {
            let mut data = json!({
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
            });
            if let Some(timer) = expire_timer {
                data["expire_timer"] = json!(timer);
            }
            json!({ "message_type": "CreateBot", "data": data })
        };

        socket.send_json(&bot(Some(604800))).await;
        socket
            .assert_receive_text_contains("\"expire_timer\":604800")
            .await;

        socket.send_json(&bot(None)).await;
        socket
            .assert_receive_text_contains("\"expire_timer\":604800")
            .await;

        socket.send_json(&bot(Some(0))).await;
        socket
            .assert_receive_text_contains("\"expire_timer\":null")
            .await;

        socket.send_json(&bot(None)).await;
        socket
            .assert_receive_text_contains("\"expire_timer\":null")
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
            .assert_receive_text_contains("\"expire_timer\":null")
            .await
    }

    #[tokio::test]
    async fn it_should_create_a_bot_without_a_name() {
        let (mut socket, _dir) = get_test_socket().await;

        socket
            .send_json(&json!({
                "message_type": "CreateBot",
                "data": {
                    "id": "bot_id",
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
        let (mut socket, _dir) = get_test_socket().await;

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
        let (mut socket, _dir) = get_test_socket().await;

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
        let (mut socket, _dir) = get_test_socket().await;

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

    #[tokio::test]
    async fn user_counts_cover_all_time_and_recent_windows() {
        use crate::{api::create_bot, db, db::conversation::BotUsers, utils::get_test_state};
        use bitpart_csml::data::{CsmlBot, CsmlFlow};

        let (state, _dir) = get_test_state().await;
        for bot_id in ["busy", "quiet", "empty"] {
            let flow = CsmlFlow::new("Default", "Default", "start: say \"Hi\" goto end", vec![]);
            let bot = CsmlBot::new(
                bot_id,
                None,
                vec![flow],
                None,
                None,
                "Default",
                None,
                None,
                None,
                None,
                None,
            );
            create_bot(bot, None, &state).await.unwrap();
        }

        let conversations = [
            ("busy", "alice", "-1 hours"),
            ("busy", "alice", "-40 days"),
            ("busy", "bob", "-10 days"),
            ("busy", "carol", "-40 days"),
            ("quiet", "alice", "-1 hours"),
        ];
        let conn = state.pool.get().await.unwrap();
        conn.interact(move |conn| {
            for (i, (bot_id, user_id, age)) in conversations.iter().enumerate() {
                conn.execute(
                    "INSERT INTO conversation \
                     (id, bot_id, channel_id, user_id, flow_id, step_id, status, updated_at) \
                     VALUES (?, ?, 'channel', ?, 'Default', 'start', 'OPEN', \
                             datetime('now', 'localtime', ?))",
                    rusqlite::params![i.to_string(), bot_id, user_id, age],
                )?;
            }
            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();
        drop(conn);

        let mut users = db::conversation::count_users(&state.pool).await.unwrap();
        users.sort_by(|a, b| a.bot_id.cmp(&b.bot_id));
        let counts = |bot_id: &str, all, last_30_days, last_7_days| BotUsers {
            bot_id: bot_id.to_owned(),
            all,
            last_30_days,
            last_7_days,
        };
        assert_eq!(
            users,
            vec![
                counts("busy", 3, 2, 1),
                counts("empty", 0, 0, 0),
                counts("quiet", 1, 1, 1),
            ]
        );
    }

    #[tokio::test]
    async fn deleting_a_bot_removes_its_conversation_records() {
        use crate::csml::data::ConversationData;
        use crate::{
            api::{create_bot, delete_bot},
            db,
            utils::get_test_state,
        };
        use bitpart_csml::data::{Client, CsmlBot, CsmlFlow, context::Context};
        use std::collections::HashMap;

        let (state, _dir) = get_test_state().await;
        let client = |bot_id: &str| Client {
            bot_id: bot_id.to_owned(),
            channel_id: "channel".to_owned(),
            user_id: "user".to_owned(),
        };

        for bot_id in ["doomed", "kept"] {
            let bot = CsmlBot::new(
                bot_id,
                None,
                vec![CsmlFlow::new(
                    "Default",
                    "Default",
                    "start: say \"Hello\" goto end",
                    vec![],
                )],
                None,
                None,
                "Default",
                None,
                None,
                None,
                None,
                None,
            );
            create_bot(bot, None, &state).await.unwrap();
            let client = client(bot_id);
            let conversation_id =
                db::conversation::create("Default", "start", &client, &state.pool)
                    .await
                    .unwrap();
            let data = ConversationData {
                conversation_id,
                request_id: "request".to_owned(),
                client: client.clone(),
                callback_url: None,
                context: Context::new(
                    HashMap::new(),
                    HashMap::new(),
                    None,
                    None,
                    "start",
                    "Default",
                    None,
                ),
                metadata: json!({}),
                messages: vec![],
                low_data: false,
                deleted: false,
            };
            db::message::create(
                &data,
                &[json!({"content_type": "text"})],
                0,
                "RECEIVE",
                &state.pool,
            )
            .await
            .unwrap();
            db::memory::create(&client, "key", &json!("value"), &state.pool)
                .await
                .unwrap();
            db::state::set(&client, "hold", "position", &json!({}), &state.pool)
                .await
                .unwrap();
        }

        assert_eq!(state.metrics.bots(), 2);
        delete_bot("doomed", &state).await.unwrap();
        assert_eq!(state.metrics.bots(), 1);

        for (bot_id, expected) in [("doomed", 0), ("kept", 1)] {
            let client = client(bot_id);
            let pool = &state.pool;
            assert_eq!(
                db::conversation::get_by_client(&client, None, None, pool)
                    .await
                    .unwrap()
                    .len(),
                expected,
                "{bot_id} conversations"
            );
            assert_eq!(
                db::memory::get_by_client(&client, None, None, pool)
                    .await
                    .unwrap()
                    .len(),
                expected,
                "{bot_id} memories"
            );
            assert_eq!(
                db::state::get_by_client(&client, pool).await.unwrap().len(),
                expected,
                "{bot_id} state"
            );
        }
        let pool = &state.pool;
        let messages = pool
            .get()
            .await
            .unwrap()
            .interact(|conn| {
                conn.query_row(
                    "SELECT bot_id, COUNT(*) FROM message JOIN conversation \
                     ON message.conversation_id = conversation.id GROUP BY bot_id",
                    [],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                )
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(messages, ("kept".to_owned(), 1));
        let orphans: i64 = pool
            .get()
            .await
            .unwrap()
            .interact(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM message WHERE conversation_id NOT IN (SELECT id FROM conversation)",
                    [],
                    |r| r.get(0),
                )
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(orphans, 0);
    }
}
