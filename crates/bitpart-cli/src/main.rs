use std::ffi::OsStr;
use std::{fs, io, path::PathBuf};

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use serde_json::{json, Value};
use tokio_tungstenite::{
    connect_async,
    tungstenite::client::IntoClientRequest,
    tungstenite::protocol::{frame::coding::CloseCode, CloseFrame, Message},
};
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

    /// add channel
    #[command(arg_required_else_help = true)]
    ChannelAdd {
        /// Channel ID
        #[arg(short, long)]
        id: String,

        /// Bot ID
        #[arg(short, long)]
        bot_id: String,
    },

    /// describe channel
    #[command(arg_required_else_help = true)]
    ChannelDescribe {
        /// Channel ID
        #[arg(short, long)]
        id: String,
    },

    /// delete channel
    #[command(arg_required_else_help = true)]
    ChannelDelete {
        /// Channel ID
        #[arg(short, long)]
        id: String,
    },

    /// list channels
    #[command()]
    ChannelList {},

    /// start channel linking
    #[command(arg_required_else_help = true)]
    ChannelLink {
        /// Channel ID
        #[arg(short, long)]
        id: String,

        /// Device name
        #[arg(short, long)]
        device_name: String,
    },

    /// complete channel linking
    #[command(arg_required_else_help = true)]
    ChannelRegister {
        /// Channel ID
        #[arg(short, long)]
        id: String,

        /// Captcha URL
        #[arg(short, long)]
        captcha: String,

        /// Phone number
        phone_number: String,
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

    /// Websocket test
    #[command()]
    Test {},
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

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    let connect = args.connect;
    let auth = args.auth;

    let url = Url::parse(&format!("ws://{}/ws", connect)).unwrap();
    let mut request = url.into_client_request().unwrap();
    let headers = request.headers_mut();
    let auth_value = HeaderValue::from_str(&auth).unwrap();
    headers.insert("Authorization", auth_value);
    let ws_stream = match connect_async(request).await {
        Ok((stream, response)) => {
            println!("Handshake for client has been completed");
            // This will be the HTTP response, same as with server this is the last moment we
            // can still access HTTP stuff.
            println!("Server response was {response:?}");
            stream
        }
        Err(e) => {
            println!("WebSocket handshake for client failed with {e}!");
            return;
        }
    };

    let (mut sender, mut receiver) = ws_stream.split();
    //receiver just prints whatever it gets
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
            "message_type": "CreateBot",
            "data" : {
                "id": id,
                "name": name,
                "default_flow": default_flow,
                "flows": flows
                }
            });
            println!("Request: {:?}", req.to_string());

            //we can ping the server for start
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
        Commands::ChannelAdd { id, bot_id } => {
            let req = json!({"message_type": "CreateChannel",
                    "data" : {
                    "id": id,
                    "bot_id": bot_id,
                    }
            });
            println!("Request: {:?}", req.to_string());

            //we can ping the server for start
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
        Commands::ChannelDescribe { id } => {
            let req = json!({"message_type": "ReadChannel", "data" : {
                "id": id
            }});
            println!("Request: {:?}", req.to_string());

            //we can ping the server for start
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
        Commands::ChannelDelete { id } => {
            let req = json!({"message_type": "DeleteChannel",
                "data" : {
                "id": id,
            }});
            println!("Request: {:?}", req.to_string());

            //we can ping the server for start
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
        Commands::ChannelList {} => {
            let req = json!({"message_type": "ListChannels"});
            println!("Request: {:?}", req.to_string());

            //we can ping the server for start
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
        Commands::ChannelLink { id, device_name } => {
            let req = json!({"message_type": "LinkChannel",
                "data" : {
                "id": id,
                "device_name": device_name
            }});
            println!("Request: {:?}", req.to_string());

            //we can ping the server for start
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
        Commands::ChannelRegister {
            id,
            phone_number,
            captcha,
        } => {
            let req = json!({"message_type" : "RegisterChannel",
                "data" : {
                "id": id,
                "phone_number": phone_number,
                "captcha": captcha,
            }});
            println!("Request: {:?}", req.to_string());

            //we can ping the server for start
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
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
            let req = json!({"message_type": "ReadBot",
                "data" : id
            });
            println!("Request: {:?}", req.to_string());

            //we can ping the server for start
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
        Commands::List {} => {
            let req = json!({ "message_type" : "ListBots" });
            println!("Request: {:?}", req.to_string());

            //we can ping the server for start
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
        Commands::Rollback { id, version } => {
            todo!();
        }
        Commands::Talk { id, message } => {
            let req = json!({ "message_type": "ChatRequest",
                "data" : {
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
                            "text": message
                        }
                    },
                    "metadata": serde_json::Value::Null,
                }
            }});
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
        Commands::Test {} => {
            let req = json!({"Register": "TEST"});
            sender
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .expect("Can not send!");
        }
    }
    tokio::spawn(async move {
        println!("Receiving!");
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(t) => {
                    println!("{}", t.as_str())
                }
                _ => println!("Unrecognized message"),
            }
        }
    })
    .await
    .unwrap();
}
