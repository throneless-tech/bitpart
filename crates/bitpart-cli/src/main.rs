// Bitpart
// Copyright (C) 2025 Throneless Tech

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use anyhow::{Context, Result};
use bitpart_common::socket::SocketMessage;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use futures_util::{Sink, SinkExt, StreamExt};
use http::HeaderValue;
use serde_json::json;
use similar::{ChangeTag, TextDiff};
use std::io;
use std::{fs, marker::Unpin, path::PathBuf};
use tokio_tungstenite::{
    connect_async,
    tungstenite::client::IntoClientRequest,
    tungstenite::protocol::{CloseFrame, Message, frame::coding::CloseCode},
};
use tracing::{debug, error};
use tracing_log::AsTrace;
use url::Url;

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

    /// Verbosity
    #[command(flatten)]
    verbose: Verbosity,

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

        /// Apps endpoint
        #[arg(short, long)]
        endpoint: Option<String>,

        /// CSML file
        #[arg(required = true)]
        path: Vec<PathBuf>,
    },

    /// delete channel
    #[command(arg_required_else_help = true)]
    ChannelDelete {
        /// Channel ID
        #[arg(short, long)]
        id: String,

        /// Bot ID
        #[arg(short, long)]
        bot_id: String,
    },

    /// list channels
    #[command()]
    ChannelList {},

    /// link a channel to a Signal account
    #[command(arg_required_else_help = true)]
    ChannelLink {
        /// Channel ID
        #[arg(short, long)]
        id: String,

        /// Bot ID
        #[arg(short, long)]
        bot_id: String,

        /// Device name
        #[arg(short, long)]
        device_name: String,
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
        #[arg(long)]
        version_a: String,

        /// Version B to compare
        #[arg(long)]
        version_b: String,
    },

    /// give a description of a bot
    #[command(arg_required_else_help = true)]
    Describe {
        /// Bot ID
        #[arg(short, long)]
        id: String,
    },

    /// list versions of a bot
    #[command(arg_required_else_help = true)]
    Versions {
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
        #[arg(long)]
        version_id: String,
    },

    /// talk to a bot
    #[command(arg_required_else_help = true)]
    Talk {
        /// Bot ID
        #[arg(short, long)]
        id: String,
    },
}

async fn send<S>(sender: &mut S, req: &serde_json::Value) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: Send + Sync + std::error::Error + 'static,
{
    sender
        .send(Message::Text(serde_json::to_string(req).unwrap().into()))
        .await
        .context("Failed to send!")
}

async fn hangup<S>(sender: &mut S) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: Send + Sync + std::error::Error + 'static,
{
    sender
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "Normal".into(),
        })))
        .await
        .context("Failed to send close message.")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(args.verbose.log_level_filter().as_trace())
        .init();
    let connect = args.connect;
    let auth = args.auth;

    let url = Url::parse(&format!("ws://{}/ws", connect)).unwrap();
    let mut request = url.into_client_request().unwrap();
    let headers = request.headers_mut();
    let auth_value = HeaderValue::from_str(&auth).unwrap();
    headers.insert("Authorization", auth_value);
    let ws_stream = match connect_async(request).await {
        Ok((stream, response)) => {
            debug!("Handshake for client has been completed");
            // This will be the HTTP response, same as with server this is the last moment we
            // can still access HTTP stuff.
            debug!("Server response was {response:?}");
            stream
        }
        Err(e) => {
            error!("WebSocket handshake for client failed with {e}!");
            return Ok(());
        }
    };

    let (mut sender, mut receiver) = ws_stream.split();
    match args.command {
        Commands::Add {
            default: default_flow,
            id,
            name,
            path,
            endpoint,
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
            "message_type": "CreateBot",
            "data" : {
                "id": id,
                "name": name,
                "default_flow": default_flow,
                "flows": flows,
                "apps_endpoint": endpoint
                }
            });
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
        Commands::ChannelDelete { id, bot_id } => {
            let req = json!({"message_type": "DeleteChannel",
                "data" : {
                "id": id,
                "bot_id": bot_id
            }});
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
        Commands::ChannelList {} => {
            let req = json!({"message_type": "ListChannels"});
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
        Commands::ChannelLink {
            id,
            bot_id,
            device_name,
        } => {
            let req = json!({"message_type": "LinkChannel",
                "data" : {
                "id": id,
                "bot_id": bot_id,
                "device_name": device_name
            }});
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
        Commands::Delete { id } => {
            let req = json!({"message_type": "DeleteBot",
                "data" : {
                    "id": id
                }
            });
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
        Commands::Diff {
            version_a,
            version_b,
        } => {
            let req = json!({"message_type": "DiffBot",
                "data" : {
                    "version_a": version_a,
                    "version_b": version_b
                }
            });
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
        Commands::Describe { id } => {
            let req = json!({"message_type": "ReadBot",
                "data" : {
                    "id": id
                }
            });
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
        Commands::List {} => {
            let req = json!({ "message_type" : "ListBots" });
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
        Commands::Rollback { id, version_id } => {
            let req = json!({"message_type": "RollbackBot",
                "data" : {
                    "id": id,
                    "version_id": version_id
                }
            });
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
        Commands::Talk { id } => {
            println!("Type 'q' to quit");
            tokio::spawn(async move {
                let mut buffer = String::new();
                loop {
                    buffer.clear();
                    io::stdin()
                        .read_line(&mut buffer)
                        .expect("Failed to read line");

                    if buffer == "q\n" {
                        break;
                    };

                    let req = json!({ "message_type": "ChatRequest",
                        "data" : {
                        "bot_id": id,
                        "apps_endpoint": "http://localhost",
                        "multibot": serde_json::Value::Null,
                        "event": {
                            "id": uuid::Uuid::new_v4().to_string(),
                            "client": {
                                "user_id": "cli",
                                "channel_id": "cli",
                                "bot_id": id
                            },
                            "payload": {
                                "content_type": "text",
                                "content": {
                                    "text": buffer.trim_end()
                                }
                            },
                            "metadata": serde_json::Value::Null,
                        }
                    }});
                    send(&mut sender, &req).await.unwrap();
                }
                hangup(&mut sender).await.unwrap();
            });
        }
        Commands::Versions { id } => {
            let req = json!({"message_type": "BotVersions",
                "data" : {
                    "id": id
                }
            });
            debug!("Request: {:?}", req.to_string());

            send(&mut sender, &req).await?;
            hangup(&mut sender).await?;
        }
    }
    //receiver just prints whatever it gets
    tokio::spawn(async move {
        debug!("Receiving!");
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(t) => {
                    let contents: SocketMessage<serde_json::Value> =
                        serde_json::from_slice(t.as_bytes()).unwrap();
                    match contents {
                        SocketMessage::Response(res) => match res.response_type {
                            res_type if res_type == "CreateBot" => {
                                println!(
                                    "Created bot {}",
                                    res.response.get("bot").and_then(|v| v.get("id")).unwrap()
                                );
                            }
                            res_type if res_type == "ReadBot" => {
                                println!(
                                    "{}",
                                    unescaper::unescape(
                                        &serde_json::to_string_pretty(
                                            res.response.get("bot").unwrap()
                                        )
                                        .unwrap(),
                                    )
                                    .unwrap()
                                );
                            }
                            res_type if res_type == "BotVersions" => {
                                res.response
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .for_each(|v| println!("{}", v.get("version_id").unwrap()));
                            }
                            res_type if res_type == "RollbackBot" => {
                                println!(
                                    "Rolled back bot {} to version {}",
                                    res.response.get("bot").and_then(|v| v.get("id")).unwrap(),
                                    res.response.get("version_id").unwrap()
                                );
                            }
                            res_type if res_type == "DiffBot" => {
                                let array = res.response.as_array().unwrap();
                                let version_a = unescaper::unescape(
                                    &serde_json::to_string_pretty(array[0].get("bot").unwrap())
                                        .unwrap(),
                                )
                                .unwrap();
                                let version_b = unescaper::unescape(
                                    &serde_json::to_string_pretty(array[1].get("bot").unwrap())
                                        .unwrap(),
                                )
                                .unwrap();
                                let diff =
                                    TextDiff::from_lines(version_a.as_str(), version_b.as_str());
                                for change in diff.iter_all_changes() {
                                    let sign = match change.tag() {
                                        ChangeTag::Delete => "-",
                                        ChangeTag::Insert => "+",
                                        ChangeTag::Equal => " ",
                                    };
                                    print!("{}{}", sign, change);
                                }
                            }
                            res_type if res_type == "DeleteBot" => {
                                println!("Deleted the bot");
                            }
                            res_type if res_type == "ListBots" => {
                                res.response
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .for_each(|v| println!("{}", v));
                            }
                            res_type if res_type == "ListChannels" => {
                                res.response.as_array().unwrap().iter().for_each(|v| {
                                    println!(
                                        "Channel: {}  for Bot: {}",
                                        v.get("channel_id").unwrap(),
                                        v.get("bot_id").unwrap(),
                                    )
                                });
                            }
                            res_type if res_type == "DeleteChannel" => {
                                println!("Deleted the channel");
                            }
                            res_type if res_type == "LinkChannel" => {
                                let _ = qr2term::print_qr(res.response.to_string());
                                println!("{}", res.response);
                            }
                            res_type if res_type == "ChatRequest" => {
                                res.response
                                    .get("messages")
                                    .unwrap()
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .for_each(|msg| {
                                        let content_type = msg
                                            .get("payload")
                                            .and_then(|v| v.get("content_type"))
                                            .unwrap()
                                            .to_string();
                                        match content_type.as_str() {
                                            "\"text\"" => println!(
                                                "{}",
                                                unescaper::unescape(
                                                    &msg.get("payload")
                                                        .and_then(|v| v.get("content"))
                                                        .and_then(|v| v.get("text"))
                                                        .unwrap()
                                                        .to_string()
                                                )
                                                .unwrap()
                                            ),
                                            _ => println!(
                                                "{}",
                                                &msg.get("payload")
                                                    .and_then(|v| v.get("content"))
                                                    .unwrap()
                                            ),
                                        }
                                    });
                            }
                            _ => {
                                error!("Unrecognized message response: {:?}", res.response);
                            }
                        },
                        SocketMessage::Error(res) => {
                            println!("{}", res.response);
                        }
                        _ => {
                            println!("Wrong socket message type")
                        }
                    }
                }
                _ => println!("Unrecognized message"),
            }
        }
    })
    .await
    .unwrap();
    Ok(())
}
