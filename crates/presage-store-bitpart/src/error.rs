use presage::{libsignal_service::protocol::SignalProtocolError, store::StoreError};
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum BitpartStoreError {
    #[error("database migration is not supported")]
    MigrationConflict,
    #[error("data store error: {0}")]
    Db(#[from] sled::Error),
    #[error("data store error: {0}")]
    DbTransaction(#[from] sled::transaction::TransactionError),
    #[cfg(feature = "encryption")]
    #[error("store cipher error: {0}")]
    StoreCipher(#[from] presage_store_cipher::StoreCipherError),
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
}

impl StoreError for BitpartStoreError {}

impl BitpartStoreError> for SignalProtocolError {
    fn from(error: BitpartStoreError) -> Self {
        error!(%error, "presage store error");
        Self::InvalidState("presage store error", error.to_string())
    }
}
