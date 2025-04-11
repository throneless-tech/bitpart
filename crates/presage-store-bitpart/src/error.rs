// presage-store-bitpart
// Copyright (C) 2025 Throneless Tech
//
// This code is derived in part from code from the Presage project:
// Copyright (C) 2024 Gabriel Féron

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

use presage::{libsignal_service::protocol::SignalProtocolError, store::StoreError};
use sea_orm::DbErr;
use std::str;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum BitpartStoreError {
    #[error("database migration is not supported")]
    MigrationConflict,
    #[error("data store error: {0}")]
    Db(#[from] DbErr),
    #[error("data store error: {0}")]
    Store(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Prost error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),
    #[error("I/O error: {0}")]
    FsExtra(#[from] fs_extra::error::Error),
    #[error("group decryption error")]
    GroupDecryption,
    #[error("No UUID")]
    NoUuid,
    #[error("Unsupported message content")]
    UnsupportedContent,
    #[error("string encoding error: {0}")]
    Utf8(#[from] str::Utf8Error),
}

impl StoreError for BitpartStoreError {}

impl From<BitpartStoreError> for SignalProtocolError {
    fn from(error: BitpartStoreError) -> Self {
        error!(%error, "presage store error");
        Self::InvalidState("presage store error", error.to_string())
    }
}
