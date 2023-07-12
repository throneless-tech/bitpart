use clap::{Parser, ValueHint};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Settings {
    #[clap(short, long, value_hint = ValueHint::FilePath)]
    config: Option<PathBuf>,
    #[clap(short, long)]
    port: Option<u16>,
}
