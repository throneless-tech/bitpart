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

use csml_interpreter::data::{Client, Context, CsmlBot, Message, MultiBot};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use super::event::SerializedEvent;
use crate::db;
use crate::error::BitpartError;

#[derive(Debug, Clone)]
pub struct SwitchBot {
    pub bot_id: String,
    pub version_id: Option<String>,
    pub flow: Option<String>,
    pub step: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct FlowTrigger {
    pub flow_id: String,
    pub step_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BotVersion {
    pub bot: CsmlBot,
    pub version_id: String,
    pub engine_version: String,
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
pub(super) enum BotOpt {
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
                    None => Err(BitpartError::Interpreter(format!(
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
                    None => Err(BitpartError::Interpreter(format!(
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
            _ => Err(BitpartError::Interpreter(
                "Invalid bot_opt format".to_owned(),
            )),
        }
    }
}
