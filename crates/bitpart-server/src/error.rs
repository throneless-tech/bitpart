use sea_orm::DbErr;
use serde_json::Error as SerdeError;

#[derive(Debug)]
pub enum BitpartError {
    Interpreter(String),
    Manager(String),
    Db(DbErr),
    Serde(SerdeError),
}

impl From<DbErr> for BitpartError {
    fn from(item: DbErr) -> Self {
        BitpartError::Db(item)
    }
}
impl From<SerdeError> for BitpartError {
    fn from(item: SerdeError) -> Self {
        BitpartError::Serde(item)
    }
}
