use base64;
use hex;
use presage;
use presage_store_bitpart::BitpartStoreError;
use sea_orm::DbErr;
use serde::Serialize;
use serde_json::Error as SerdeError;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BitpartError {
    #[error("Interpreter error: `{0}`")]
    Interpreter(String),
    #[error("Manager error: `{0}`")]
    Manager(String),
    #[error("Database error: `{0}`")]
    Db(#[from] DbErr),
    #[error("I/O error: `{0}`")]
    Io(#[from] io::Error),
    #[error("Presage store error")]
    PresageStore,
    #[error("Attachment error: `{0}`")]
    Attachment(#[from] presage::libsignal_service::sender::AttachmentUploadError),
    #[error("Serialization/deserialization error")]
    Serde(#[from] SerdeError),
    #[error("Signal error: `{0}`")]
    Signal(String),
    #[error("Decode base64 error: `{0}`")]
    DecodeBase64(#[from] base64::DecodeError),
    #[error("Decode hex error: `{0}`")]
    DecodeHex(#[from] hex::FromHexError),
    #[error("Signal error: `{0}`")]
    SignalManager(#[from] anyhow::Error), //TODO actually swap out the errors in the signal channel file
    #[error("Signal storage error: `{0}`")]
    SignalStore(#[from] BitpartStoreError),
    #[error("Websocket close")]
    WebsocketClose,
    #[error("Channel error: `{0}`")]
    ChannelError(#[from] futures::channel::oneshot::Canceled),
}

impl<S: std::error::Error> From<presage::Error<S>> for BitpartError {
    fn from(_err: presage::Error<S>) -> Self {
        Self::PresageStore
    }
}
