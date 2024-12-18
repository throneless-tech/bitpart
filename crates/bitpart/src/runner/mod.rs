pub mod channels;

use clap::Args;
use clap_verbosity_flag::Verbosity;
use presage::model::identity::OnNewIdentity;
use presage_store_bitpart::{BitpartStore, MigrationConflictStrategy};
use sea_orm::DatabaseConnection;

use crate::error::BitpartError;

#[derive(Debug, Args)]
pub struct RunnerArgs {
    /// Verbosity
    #[command(flatten)]
    verbose: Verbosity,

    /// API authentication token
    #[arg(short, long)]
    auth: String,

    /// Unix socket to connect to
    #[arg(short, long)]
    connect: String,
}

async fn start_channel(id: &str, db: &DatabaseConnection) -> Result<(), BitpartError> {
    let store = BitpartStore::open(
        id,
        db,
        MigrationConflictStrategy::Raise,
        OnNewIdentity::Trust,
    )
    .await?;

    channels::signal::receive_from(store, true)
        .await
        .map_err(|e| BitpartError::Signal(e))
}
