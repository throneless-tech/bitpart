use sea_orm::DatabaseConnection;
pub use sea_orm_migration::prelude::*;

use crate::error::BitpartError;

mod m20240801_000001_create_bot;
mod m20240801_000002_create_conversation;
mod m20240801_000003_create_memory;
mod m20240801_000004_create_message;
mod m20240801_000005_create_state;
mod m20240801_000006_create_channel;
mod m20240801_000007_create_channel_state;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240801_000001_create_bot::Migration),
            Box::new(m20240801_000002_create_conversation::Migration),
            Box::new(m20240801_000003_create_memory::Migration),
            Box::new(m20240801_000004_create_message::Migration),
            Box::new(m20240801_000005_create_state::Migration),
            Box::new(m20240801_000006_create_channel::Migration),
            Box::new(m20240801_000007_create_channel_state::Migration),
        ]
    }
}

pub async fn migrate(db: &DatabaseConnection) -> Result<(), BitpartError> {
    Migrator::up(db, None).await?;
    Ok(())
}
