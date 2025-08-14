use csml_interpreter::data::{Client, CsmlBot, Event, MultiBot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{BitpartError, BitpartErrorKind};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FlowTrigger {
    pub flow_id: String,
    pub step_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum BotOpt {
    #[serde(rename = "bot")]
    CsmlBot(Box<CsmlBot>),
    #[serde(rename = "version_id")]
    Id {
        version_id: String,
        bot_id: String,
        apps_endpoint: Option<String>,
        multibot: Option<Vec<MultiBot>>,
    },
    #[serde(rename = "bot_id")]
    BotId {
        bot_id: String,
        apps_endpoint: Option<String>,
        multibot: Option<Vec<MultiBot>>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Request {
    pub bot: Option<CsmlBot>,
    pub bot_id: Option<String>,
    pub version_id: Option<String>,
    #[serde(alias = "fn_endpoint")]
    pub apps_endpoint: Option<String>,
    pub multibot: Option<Vec<MultiBot>>,
    pub event: SerializedEvent,
}

impl TryInto<BotOpt> for Request {
    type Error = BitpartError;

    fn try_into(self) -> Result<BotOpt, Self::Error> {
        match self {
            // Bot
            Request {
                bot: Some(mut csml_bot),
                multibot,
                ..
            } => {
                csml_bot.multibot = multibot;

                Ok(BotOpt::CsmlBot(Box::new(csml_bot)))
            }

            // version id
            Request {
                version_id: Some(version_id),
                bot_id: Some(bot_id),
                apps_endpoint,
                multibot,
                ..
            } => Ok(BotOpt::Id {
                version_id,
                bot_id,
                apps_endpoint,
                multibot,
            }),

            // get bot by id will search for the last version id
            Request {
                bot_id: Some(bot_id),
                apps_endpoint,
                multibot,
                ..
            } => Ok(BotOpt::BotId {
                bot_id,
                apps_endpoint,
                multibot,
            }),
            _ => Err(BitpartErrorKind::Interpreter("Invalid bot_opt format".to_owned()).into()),
        }
    }
}

impl TryInto<BotOpt> for &Request {
    type Error = BitpartError;

    fn try_into(self) -> Result<BotOpt, Self::Error> {
        match self.clone() {
            // Bot
            Request {
                bot: Some(csml_bot),
                multibot,
                ..
            } => {
                let mut csml_bot = csml_bot.to_owned();
                csml_bot.multibot = multibot.to_owned();

                Ok(BotOpt::CsmlBot(Box::new(csml_bot)))
            }

            // version id
            Request {
                version_id: Some(version_id),
                bot_id: Some(bot_id),
                apps_endpoint,
                multibot,
                ..
            } => Ok(BotOpt::Id {
                version_id: version_id.to_owned(),
                bot_id: bot_id.to_owned(),
                apps_endpoint: apps_endpoint.to_owned(),
                multibot: multibot.to_owned(),
            }),

            // get bot by id will search for the last version id
            Request {
                bot_id: Some(bot_id),
                apps_endpoint,
                multibot,
                ..
            } => Ok(BotOpt::BotId {
                bot_id: bot_id.to_owned(),
                apps_endpoint: apps_endpoint.to_owned(),
                multibot: multibot.to_owned(),
            }),
            _ => Err(BitpartErrorKind::Interpreter("Invalid bot_opt format".to_owned()).into()),
        }
    }
}

fn get_event_content(content_type: &str, metadata: &Value) -> Result<String, BitpartError> {
    match content_type {
        file if ["file", "audio", "video", "image", "url"].contains(&file) => {
            if let Some(val) = metadata["url"].as_str() {
                Ok(val.to_string())
            } else {
                Err(BitpartErrorKind::Interpreter("no url content in event".to_owned()).into())
            }
        }
        "payload" => {
            if let Some(val) = metadata["payload"].as_str() {
                Ok(val.to_string())
            } else {
                Err(BitpartErrorKind::Interpreter("no payload content in event".to_owned()).into())
            }
        }
        "text" => {
            if let Some(val) = metadata["text"].as_str() {
                Ok(val.to_string())
            } else {
                Err(BitpartErrorKind::Interpreter("no text content in event".to_owned()).into())
            }
        }
        "regex" => {
            if let Some(val) = metadata["payload"].as_str() {
                Ok(val.to_string())
            } else {
                Err(BitpartErrorKind::Interpreter(
                    "invalid payload for event type regex".to_owned(),
                )
                .into())
            }
        }
        "flow_trigger" => match serde_json::from_value::<FlowTrigger>(metadata.clone()) {
            Ok(_) => Ok(metadata.to_string()),
            Err(_) => Err(BitpartErrorKind::Interpreter(
                "invalid content for event type flow_trigger: expect flow_id and optional step_id"
                    .to_owned(),
            )
            .into()),
        },
        content_type => Err(BitpartErrorKind::Interpreter(format!(
            "{} is not a valid content_type",
            content_type
        ))
        .into()),
    }
}

fn request_to_event(request: &SerializedEvent) -> Result<Event, BitpartError> {
    let step_limit = request.step_limit;
    let json_event = json!(request);

    let content_type = match json_event["payload"]["content_type"].as_str() {
        Some(content_type) => content_type.to_string(),
        None => {
            return Err(BitpartErrorKind::Interpreter(
                "no content_type in event payload".to_owned(),
            )
            .into());
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
        secure: json_event["payload"]["secure"].as_bool().unwrap_or(false),
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
