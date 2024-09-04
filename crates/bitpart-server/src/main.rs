use clap::Parser;

use bitpart_server::error::BitpartError;

/// The Bitpart interpreter
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Connection URI for postgres database
    #[arg(short, long)]
    connect: String,

    /// Directory of CSML files
    #[arg(short, long)]
    directory: String,
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTION
////////////////////////////////////////////////////////////////////////////////

#[tokio::main]
async fn main() -> Result<(), BitpartError> {
    let args = Args::parse();

    println!("{}", args.connect);
    println!("{}", args.directory);

    Ok(())
}
