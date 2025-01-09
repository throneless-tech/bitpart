use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use presage_store_bitpart::BitpartStoreError;
use sea_orm::DbErr;
use serde_json::Error as SerdeError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BitpartError {
    #[error("Interpreter error: `{0}`")]
    Interpreter(String),
    #[error("Manager error: `{0}`")]
    Manager(String),
    #[error("Database error: `{0}`")]
    Db(#[from] DbErr),
    #[error("Serialization/deserialization error")]
    Serde(#[from] SerdeError),
    #[error("Signal error: `{0}`")]
    Signal(#[from] anyhow::Error), //TODO actually swap out the errors in the signal channel file
    #[error("Signal storage error: `{0}`")]
    SignalStore(#[from] BitpartStoreError),
    #[error("Websocket close")]
    WebsocketClose,
}

impl IntoResponse for BitpartError {
    fn into_response(self) -> Response {
        println!("{:?}", self.to_string());
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
