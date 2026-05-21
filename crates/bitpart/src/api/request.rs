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

use bitpart_common::{csml::Request, db::Pool, error::Result};

use crate::csml::conversation;

pub async fn process_request(
    body: &Request,
    pool: &Pool,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    match conversation::start(body, pool).await {
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
