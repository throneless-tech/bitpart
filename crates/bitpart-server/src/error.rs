use aide::OperationIo;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sea_orm::DbErr;
use serde_json::Error as SerdeError;
use thiserror::Error;

#[derive(Debug, Error, OperationIo)]
pub enum BitpartError {
    #[error("Interpreter error: `{0}`")]
    Interpreter(String),
    #[error("Manager error: `{0}`")]
    Manager(String),
    #[error("Database error")]
    Db(#[from] DbErr),
    #[error("Serialization/deserialization error")]
    Serde(#[from] SerdeError),
}

impl IntoResponse for BitpartError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self).into_response()
    }
}
