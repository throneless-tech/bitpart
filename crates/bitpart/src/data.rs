use csml_interpreter::data::{Client, Context, CsmlBot, CsmlFlow, Message, Module, MultiBot};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::{db, error::BitpartError};

#[derive(Debug, Clone)]
pub struct SwitchBot {
    pub bot_id: String,
    pub version_id: Option<String>,
    pub flow: Option<String>,
    pub step: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FlowTrigger {
    pub flow_id: String,
    pub step_id: Option<String>,
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

#[derive(Serialize, Deserialize, Debug)]
pub struct BotVersion {
    pub bot: CsmlBot,
    pub version_id: String,
    pub engine_version: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BotSummary {
    pub bot: CsmlBot,
    pub version_id: String,
    pub engine_version: String,
}

// impl IntoResponse for BotVersion {
//     fn into_response(self) -> axum::response::Response {
//         (StatusCode::CREATED, self).into_response()
//     }
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedCsmlBot {
    pub id: String,
    pub name: String,
    pub flows: Vec<CsmlFlow>,
    pub native_components: Option<String>, // serde_json::Map<String, serde_json::Value>
    pub custom_components: Option<String>, // serde_json::Value
    pub default_flow: String,
    pub no_interruption_delay: Option<i32>,
    pub env: Option<String>,
    pub modules: Option<Vec<Module>>,
}

impl SerializedCsmlBot {
    pub fn to_bot(&self) -> CsmlBot {
        CsmlBot {
            id: self.id.to_owned(),
            name: self.name.to_owned(),
            apps_endpoint: None,
            flows: self.flows.to_owned(),
            native_components: {
                match self.native_components.to_owned() {
                    Some(value) => match serde_json::from_str(&value) {
                        Ok(serde_json::Value::Object(map)) => Some(map),
                        _ => unreachable!(),
                    },
                    None => None,
                }
            },
            custom_components: {
                match self.custom_components.to_owned() {
                    Some(value) => match serde_json::from_str(&value) {
                        Ok(value) => Some(value),
                        Err(_e) => unreachable!(),
                    },
                    None => None,
                }
            },
            default_flow: self.default_flow.to_owned(),
            bot_ast: None,
            no_interruption_delay: self.no_interruption_delay,
            env: self.env.as_ref().map(|e| serde_json::from_str(&e).unwrap()),
            modules: self.modules.to_owned(),
            multibot: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConversationData {
    pub conversation_id: String,
    pub request_id: String,
    pub client: Client,
    pub callback_url: Option<String>,
    pub context: Context,
    pub metadata: serde_json::Value,
    pub messages: Vec<Message>,
    pub ttl: Option<chrono::Duration>,
    pub low_data: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum BotOpt {
    #[serde(rename = "bot")]
    CsmlBot(CsmlBot),
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

impl BotOpt {
    pub async fn search_bot(&self, db: &DatabaseConnection) -> Result<CsmlBot, BitpartError> {
        match self {
            BotOpt::CsmlBot(csml_bot) => Ok(csml_bot.to_owned()),
            BotOpt::BotId {
                bot_id,
                apps_endpoint,
                multibot,
            } => {
                let bot_version = db::bot::get_latest_by_bot_id(&bot_id, db).await?;

                match bot_version {
                    Some(mut bot_version) => {
                        bot_version.bot.apps_endpoint = apps_endpoint.to_owned();
                        bot_version.bot.multibot = multibot.to_owned();
                        Ok(bot_version.bot)
                    }
                    None => Err(BitpartError::Manager(format!(
                        "bot ({}) not found in db",
                        bot_id
                    ))),
                }
            }
            BotOpt::Id {
                version_id,
                bot_id: _,
                apps_endpoint,
                multibot,
            } => {
                let bot_version = db::bot::get_by_id(&version_id, db).await?;

                match bot_version {
                    Some(mut bot_version) => {
                        bot_version.bot.apps_endpoint = apps_endpoint.to_owned();
                        bot_version.bot.multibot = multibot.to_owned();
                        Ok(bot_version.bot)
                    }
                    None => Err(BitpartError::Manager(format!(
                        "bot version ({}) not found in db",
                        version_id
                    ))),
                }
            }
        }
    }
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

impl Request {
    pub fn get_bot_opt(&self) -> Result<BotOpt, BitpartError> {
        match self.clone() {
            // Bot
            Request {
                bot: Some(mut csml_bot),
                multibot,
                ..
            } => {
                csml_bot.multibot = multibot;

                Ok(BotOpt::CsmlBot(csml_bot))
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

            _ => Err(BitpartError::Interpreter(
                "Invalid bot_opt format".to_owned(),
            )),
        }
    }
}
