// Bitpart
// Copyright (C) 2025 Throneless Tech
//
// This code is derived in part from code from the CSML project:
// Copyright (C) 2020 CSML

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

use csml_interpreter::data::{Client, Event};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::data::FlowTrigger;
use crate::error::BitpartError;

fn get_event_content(content_type: &str, metadata: &Value) -> Result<String, BitpartError> {
    match content_type {
        file if ["file", "audio", "video", "image", "url"].contains(&file) => {
            if let Some(val) = metadata["url"].as_str() {
                Ok(val.to_string())
            } else {
                Err(BitpartError::Interpreter(
                    "no url content in event".to_owned(),
                ))
            }
        }
        payload if payload == "payload" => {
            if let Some(val) = metadata["payload"].as_str() {
                Ok(val.to_string())
            } else {
                Err(BitpartError::Interpreter(
                    "no payload content in event".to_owned(),
                ))
            }
        }
        text if text == "text" => {
            if let Some(val) = metadata["text"].as_str() {
                Ok(val.to_string())
            } else {
                Err(BitpartError::Interpreter(
                    "no text content in event".to_owned(),
                ))
            }
        }
        regex if regex == "regex" => {
            if let Some(val) = metadata["payload"].as_str() {
                Ok(val.to_string())
            } else {
                Err(BitpartError::Interpreter(
                    "invalid payload for event type regex".to_owned(),
                ))
            }
        }
        flow_trigger if flow_trigger == "flow_trigger" => {
            match serde_json::from_value::<FlowTrigger>(metadata.clone()) {
                Ok(_flow_trigger) => {
                    Ok(metadata.to_string())
                }
                Err(_) => {
                    Err(BitpartError::Interpreter(
                        "invalid content for event type flow_trigger: expect flow_id and optional step_id".to_owned(),
                    ))
                }
            }
        }
        content_type => Err(BitpartError::Interpreter(format!(
            "{} is not a valid content_type",
            content_type
        ))),
    }
}

fn request_to_event(request: &SerializedEvent) -> Result<Event, BitpartError> {
    let step_limit = request.step_limit;
    let json_event = json!(request);

    let content_type = match json_event["payload"]["content_type"].as_str() {
        Some(content_type) => content_type.to_string(),
        None => {
            return Err(BitpartError::Interpreter(
                "no content_type in event payload".to_owned(),
            ));
        }
    };
    let content = json_event["payload"]["content"].to_owned();

    let content_value = get_event_content(&content_type, &content)?;

    Ok(Event {
        content_type,
        content_value,
        content,
        ttl_duration: json_event["ttl_duration"].as_i64(),
        low_data_mode: json_event["low_data_mode"].as_bool(),
        step_limit,
        secure: json_event["payload"]["secure"].as_bool().unwrap_or(true), // we default to secure
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEvent {
    pub id: String,
    pub client: Client,
    pub metadata: serde_json::Value,
    pub payload: serde_json::Value,
    pub step_limit: Option<usize>,
    pub callback_url: Option<String>,
}

impl TryFrom<&SerializedEvent> for Event {
    type Error = BitpartError;

    fn try_from(val: &SerializedEvent) -> Result<Event, Self::Error> {
        request_to_event(val)
    }
}

impl TryFrom<SerializedEvent> for Event {
    type Error = BitpartError;

    fn try_from(val: SerializedEvent) -> Result<Event, Self::Error> {
        request_to_event(&val)
    }
}
