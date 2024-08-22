use crate::commands::{Interpreter, Operator, Runner};
mod settings;
//mod commands::{Operator, Runner, Interpreter};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about, long_about=None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[clap(name = "interpreter")]
    Interpreter(interpreter::Command),
    #[clap(name = "operator")]
    Operator(operator::Command),
    #[clap(name = "runner")]
    Runner(runner::Command),
}

fn main() {
    let args = Cli::parse();
    match args.command {
        Commands::Interpreter(command) => command.execute(),
        Commands::Operator(command) => command.execute(),
        Commands::Run(command) => command.execute(),
    }
    println!("Hello, world!");
}
