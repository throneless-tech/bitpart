use csml_interpreter::data::CsmlBot;
use serde::{Deserialize, Serialize};

use crate::csml::Request;

#[derive(Debug, Serialize, Deserialize)]
pub struct Paginate {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response<S: Serialize> {
    pub response_type: String,
    pub response: S,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "message_type", content = "data")]
pub enum SocketMessage<S: Serialize> {
    CreateBot(Box<CsmlBot>),
    ReadBot {
        id: String,
    },
    BotVersions {
        id: String,
        options: Option<Paginate>,
    },
    RollbackBot {
        id: String,
        version_id: String,
    },
    DiffBot {
        version_a: String,
        version_b: String,
    },
    DeleteBot {
        id: String,
    },
    ListBots(Option<Paginate>),
    CreateChannel {
        id: String,
        bot_id: String,
    },
    ReadChannel {
        id: String,
        bot_id: String,
    },
    ListChannels(Option<Paginate>),
    DeleteChannel {
        id: String,
        bot_id: String,
    },
    LinkChannel {
        id: String,
        bot_id: String,
        device_name: String,
    },
    ChatRequest(Box<Request>),
    Response(Response<S>),
    Error(Response<S>),
}
