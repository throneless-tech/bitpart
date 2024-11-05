use std::ffi::OsStr;
use std::{fs, io, path::PathBuf};

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::blocking::Client;
use serde_json::{json, Value};

/// The Bitpart CLI
#[derive(Debug, Parser)] // requires `derive` feature
#[command(version, about, long_about = None)]
struct Cli {
    /// API authentication token
    #[arg(short, long)]
    auth: String,

    /// IP address and port to connect to
    #[arg(short, long)]
    connect: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// add a bot
    #[command(arg_required_else_help = true)]
    Add {
        /// Bot ID
        #[arg(short, long)]
        id: String,

        /// Bot Name
        #[arg(short, long)]
        name: String,

        /// Default flow
        #[arg(short, long)]
        default: String,

        /// CSML file
        #[arg(required = true)]
        path: Vec<PathBuf>,
    },

    /// delete a bot
    #[command(arg_required_else_help = true)]
    Delete {
        /// Bot ID
        #[arg(short, long)]
        id: String,
    },

    /// Show the differences between two versions of a bot
    #[command(arg_required_else_help = true)]
    Diff {
        /// Version A to compare
        #[arg(short, long)]
        version_a: String,

        /// Version B to compare
        #[arg(short, long)]
        version_b: String,
    },

    /// give a description of a bot
    #[command(arg_required_else_help = true)]
    Describe {
        /// Bot ID
        #[arg(short, long)]
        id: String,
    },

    /// list bots
    #[command()]
    List {},

    /// Rollback a bot to a previous version
    #[command(arg_required_else_help = true)]
    Rollback {
        /// Bot ID
        #[arg(short, long)]
        id: String,

        /// Target version
        #[arg(short, long)]
        version: String,
    },

    /// talk to a bot
    #[command(arg_required_else_help = true)]
    Talk {
        /// Bot ID
        #[arg(short, long)]
        id: String,

        message: String,
    },
}

fn find_csml(path: &str) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(path)?
        .filter_map(|res| match res.ok()?.path() {
            path if path.extension() == Some(OsStr::new("csml")) => Some(path),
            _ => None,
        })
        .collect::<Vec<_>>();

    Ok(entries)
}

fn main() {
    let args = Cli::parse();

    let connect = args.connect;
    let auth = args.auth;

    match args.command {
        Commands::Add {
            default: default_flow,
            id,
            name,
            path,
        } => {
            let flows = path
                .iter()
                .map(|p| {
                    let basename = p.file_stem().unwrap().to_str();
                    let content = fs::read_to_string(p).unwrap();
                    json!({
                        "id": basename,
                        "name": basename,
                        "content": content,
                        "commands": []
                    })
                })
                .collect::<Vec<serde_json::Value>>();
            let req = json!({
                "id": id,
                "name": name,
                "default_flow": default_flow,
                "flows": flows
            });
            println!("Request: {:?}", req.to_string());

            let client = Client::new();
            let res = client
                .post(format!("{connect}/api/v1/bots"))
                .json(&req)
                .header("Authorization", auth)
                .send()
                .unwrap();
            println!("Response: {:?}", res);
        }
        Commands::Delete { id } => {
            todo!();
        }
        Commands::Diff {
            version_a,
            version_b,
        } => {
            todo!();
        }
        Commands::Describe { id } => {
            let client = Client::new();
            let res = client
                .get(format!("{connect}/api/v1/bots/{id}/versions"))
                .header("Authorization", auth)
                .send()
                .unwrap();
            println!("Response: {:?}", res.text());
        }
        Commands::List {} => {
            let client = Client::new();
            let res: Value = client
                .get(format!("{connect}/api/v1/bots"))
                .header("Authorization", auth)
                .send()
                .unwrap()
                .json()
                .unwrap();
            println!("{res}");
        }
        Commands::Rollback { id, version } => {
            todo!();
        }
        Commands::Talk { id, message } => {
            let req = json!({
                "bot_id": id,
                "apps_endpoint": "http://localhost",
                "multibot": serde_json::Value::Null,
                "event": {
                    "id": "request_id",
                    "client": {
                        "user_id": "cli",
                        "channel_id": "cli",
                        "bot_id": id
                    },
                    "payload": {
                        "content_type": "text",
                        "content": {
                            "text": "todo"
                        }
                    },
                    "metadata": serde_json::Value::Null,
                }
            });

            let client = Client::new();
            let res: Value = client
                .post(format!("{connect}/api/v1/requests"))
                .json(&req)
                .header("Authorization", auth)
                .send()
                .unwrap()
                .json()
                .unwrap();
            println!("{res}");
        }
    }
}
