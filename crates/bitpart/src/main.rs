pub mod error;
pub mod runner;
pub mod server;
pub mod utils;

use clap::{Parser, Subcommand};

use error::BitpartError;

/// The Bitpart server
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Runner(runner::RunnerArgs),
    Server(server::ServerArgs),
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTION
////////////////////////////////////////////////////////////////////////////////

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), BitpartError> {
    let args = Cli::parse();
    match args.command {
        Commands::Runner(_runner) => {
            todo!("Implement runner");
        }

        Commands::Server(server) => server::init_server(server).await,
    }
}
