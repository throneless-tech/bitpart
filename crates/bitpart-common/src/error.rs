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

use base64;
use bincode;
use figment;
use futures;
use hex;
use opentelemetry_otlp;
use presage;
use presage_store_bitpart::BitpartStoreError;
use prost;
use sea_orm::DbErr;
use serde_json::Error as SerdeError;
use std::{array, io};
use thiserror::Error;
use tokio;
use uuid;

#[derive(Debug, Error)]
pub enum BitpartError {
    #[error("API error: `{0}`")]
    Api(String),
    #[error("Interpreter error: `{0}`")]
    Interpreter(String),
    #[error("Database error: `{0}`")]
    Db(#[from] DbErr),
    #[error("I/O error: `{0}`")]
    Io(#[from] io::Error),
    #[error("Directory error: `{0}`")]
    Directory(String),
    #[error("Figment error: `{0}`")]
    Figment(#[from] figment::Error),
    #[error("Channel Receive error: `{0}`")]
    ChannelRecv(#[from] tokio::sync::oneshot::error::RecvError),
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
    #[error("Channel Canceled error: `{0}`")]
    ChannelCanceled(#[from] futures::channel::oneshot::Canceled),
    #[error("Signal Recipient error: `{0}`")]
    SignalRecipient(#[from] array::TryFromSliceError),
    #[error("Signal Message error: `{0}`")]
    SignalMessage(#[from] uuid::Error),
    #[error("OpenTelemetry build error: `{0}`")]
    OpenTelemetry(#[from] opentelemetry_otlp::ExporterBuildError),
    #[error("Protocol Buffers error: `{0}`")]
    ProtocolBuffers(#[from] prost::UnknownEnumValue),
    #[error("Bincode error: `{0}`")]
    Bincode(#[from] bincode::Error),
}

impl<S: std::error::Error> From<presage::Error<S>> for BitpartError {
    fn from(_err: presage::Error<S>) -> Self {
        Self::PresageStore
    }
}

pub type Result<T> = std::result::Result<T, BitpartError>;
