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

use bitpart_common::{
    csml::BotOpt,
    error::{BitpartErrorKind, Result},
};
use csml_interpreter::data::{Client, Context, CsmlBot, Message};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::db;

#[derive(Debug, Clone)]
pub struct SwitchBot {
    pub bot_id: String,
    pub version_id: Option<String>,
    pub flow: Option<String>,
    pub step: String,
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

pub async fn search_bot(bot: &BotOpt, db: &DatabaseConnection) -> Result<Box<CsmlBot>> {
    match bot {
        BotOpt::CsmlBot(csml_bot) => Ok(csml_bot.to_owned()),
        BotOpt::BotId {
            bot_id,
            apps_endpoint: _,
            multibot: _,
        } => {
            let bot_version = db::bot::get_latest_by_bot_id(bot_id, db).await?;

            match bot_version {
                Some(bot_version) => {
                    // bot_version.bot.apps_endpoint = apps_endpoint.to_owned();
                    // bot_version.bot.multibot = multibot.to_owned();
                    Ok(Box::new(bot_version.bot))
                }
                None => Err(BitpartErrorKind::Interpreter(format!(
                    "bot ({}) not found in db",
                    bot_id
                ))
                .into()),
            }
        }
        BotOpt::Id {
            version_id,
            bot_id: _,
            apps_endpoint: _,
            multibot: _,
        } => {
            let bot_version = db::bot::get_by_id(version_id, db).await?;

            match bot_version {
                Some(bot_version) => {
                    // bot_version.bot.apps_endpoint = apps_endpoint.to_owned();
                    // bot_version.bot.multibot = multibot.to_owned();
                    Ok(Box::new(bot_version.bot))
                }
                None => Err(BitpartErrorKind::Interpreter(format!(
                    "bot version ({}) not found in db",
                    version_id
                ))
                .into()),
            }
        }
    }
}
