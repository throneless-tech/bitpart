pub mod channels;

use clap::Args;
use clap_verbosity_flag::Verbosity;
use futures::{
    sink::SinkExt,
    stream::{FuturesUnordered, StreamExt},
};
use presage::model::identity::OnNewIdentity;
use presage_store_bitpart::{BitpartStore, MigrationConflictStrategy};
use sea_orm::DatabaseConnection;
use std::ops::ControlFlow;
use std::time::Instant;
use tokio_tungstenite::{
    connect_async,
    tungstenite::client::IntoClientRequest,
    tungstenite::protocol::{frame::coding::CloseCode, CloseFrame, Message},
    tungstenite::Utf8Bytes,
};
use tracing_log::AsTrace;
use url;

use crate::error::BitpartError;

const N_CLIENTS: usize = 2; //set to desired number

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

//creates a client. quietly exits on failure.
async fn spawn_client(server: String, auth: String, who: usize) {
    let url = url::Url::parse(&format!("ws://{}/api/v1/ws", server)).unwrap();
    let mut request = url.into_client_request().unwrap();
    let request = Request::builder()
        .method("GET")
        .uri(format!("ws://{}/api/v1/ws", server))
        .header("Authorization", auth)
        .body(())
        .unwrap();
    let ws_stream = match connect_async(request).await {
        Ok((stream, response)) => {
            println!("Handshake for client {who} has been completed");
            // This will be the HTTP response, same as with server this is the last moment we
            // can still access HTTP stuff.
            println!("Server response was {response:?}");
            stream
        }
        Err(e) => {
            println!("WebSocket handshake for client {who} failed with {e}!");
            return;
        }
    };

    let (mut sender, mut receiver) = ws_stream.split();

    //we can ping the server for start
    sender
        .send(Message::Ping("Hello, Server!".into()))
        .await
        .expect("Can not send!");

    //spawn an async sender to push some more messages into the server
    let mut send_task = tokio::spawn(async move {
        for i in 1..30 {
            // In any websocket error, break loop.
            if sender
                .send(Message::Text(format!("Message number {i}...").into()))
                .await
                .is_err()
            {
                //just as with server, if send fails there is nothing we can do but exit.
                return;
            }

            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        // When we are done we may want our client to close connection cleanly.
        println!("Sending close to {who}...");
        if let Err(e) = sender
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: Utf8Bytes::from_static("Goodbye"),
            })))
            .await
        {
            println!("Could not send Close due to {e:?}, probably it is ok?");
        };
    });

    //receiver just prints whatever it gets
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            // print message and break if instructed to do so
            if process_message(msg, who).is_break() {
                break;
            }
        }
    });

    //wait for either task to finish and kill the other task
    tokio::select! {
        _ = (&mut send_task) => {
            recv_task.abort();
        },
        _ = (&mut recv_task) => {
            send_task.abort();
        }
    }
}

/// Function to handle messages we get (with a slight twist that Frame variant is visible
/// since we are working with the underlying tungstenite library directly without axum here).
fn process_message(msg: Message, who: usize) -> ControlFlow<(), ()> {
    match msg {
        Message::Text(t) => {
            println!(">>> {who} got str: {t:?}");
        }
        Message::Binary(d) => {
            println!(">>> {} got {} bytes: {:?}", who, d.len(), d);
        }
        Message::Close(c) => {
            if let Some(cf) = c {
                println!(
                    ">>> {} got close with code {} and reason `{}`",
                    who, cf.code, cf.reason
                );
            } else {
                println!(">>> {who} somehow got close message without CloseFrame");
            }
            return ControlFlow::Break(());
        }

        Message::Pong(v) => {
            println!(">>> {who} got pong with {v:?}");
        }
        // Just as with axum server, the underlying tungstenite websocket library
        // will handle Ping for you automagically by replying with Pong and copying the
        // v according to spec. But if you need the contents of the pings you can see them here.
        Message::Ping(v) => {
            println!(">>> {who} got ping with {v:?}");
        }

        Message::Frame(_) => {
            unreachable!("This is never supposed to happen")
        }
    }
    ControlFlow::Continue(())
}

pub async fn init_runner(runner: RunnerArgs) -> Result<(), BitpartError> {
    tracing_subscriber::fmt()
        .with_max_level(runner.verbose.log_level_filter().as_trace())
        .init();

    let start_time = Instant::now();
    //spawn several clients that will concurrently talk to the server
    let mut clients = (0..N_CLIENTS)
        .map(|cli| {
            tokio::spawn(spawn_client(
                runner.connect.clone(),
                runner.auth.clone(),
                cli,
            ))
        })
        .collect::<FuturesUnordered<_>>();

    //wait for all our clients to exit
    while clients.next().await.is_some() {}

    let end_time = Instant::now();

    //total time should be the same no matter how many clients we spawn
    println!(
        "Total time taken {:#?} with {N_CLIENTS} concurrent clients, should be about 6.45 seconds.",
        end_time - start_time
    );
    println!("Runner is..um... running!");
    Ok(())
}
