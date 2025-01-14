use csml_interpreter::data::CsmlBot;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::data::Request;

#[derive(Serialize, Deserialize)]
pub struct PaginateMessage {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}
#[derive(Serialize, Deserialize)]
pub struct CreateChannelMessage {
    pub id: String,
    pub bot_id: String,
}
#[derive(Serialize, Deserialize)]
pub struct LinkChannelMessage {
    pub id: String,
    pub device_name: String,
}
#[derive(Serialize, Deserialize)]
pub struct AddDeviceChannelMessage {
    pub id: String,
    pub url: Url,
}

#[derive(Serialize, Deserialize)]
pub enum SocketMessage {
    CreateBot(CsmlBot),
    ReadBot(String),
    DeleteBot(String),
    ListBots,
    CreateChannel(CreateChannelMessage),
    ReadChannel(String),
    ListChannels(PaginateMessage),
    DeleteChannel(String),
    LinkChannel(LinkChannelMessage),
    RegisterChannel {
        id: String,
        phone_number: String,
        captcha: String,
    },
    ChatRequest(Request),
    Error(String),
}
