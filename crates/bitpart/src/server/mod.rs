pub mod actions;
pub mod api;
pub mod conversation;
pub mod data;
pub mod db;
pub mod event;

use axum::extract::connect_info::ConnectInfo;
use axum::extract::ws::CloseFrame;
use axum::{
    debug_handler,
    extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Router,
};
use axum_extra::TypedHeader;
use bytes::Bytes;
use clap::Args;
use clap_verbosity_flag::Verbosity;
use futures::{sink::SinkExt, stream::StreamExt};
use sea_orm::{ConnectionTrait, Database};
use std::borrow::Cow;
use std::net::SocketAddr;
use std::ops::ControlFlow;
use tracing_log::AsTrace;

use crate::error::BitpartError;

#[derive(Debug, Args)]
pub struct ServerArgs {
    /// Verbosity
    #[command(flatten)]
    verbose: Verbosity,

    /// API authentication token
    #[arg(short, long)]
    auth: String,

    /// IP address and port to bind to
    #[arg(short, long)]
    bind: String,

    /// Path to sqlcipher database file
    #[arg(short, long)]
    database: String,

    /// Database encryption key
    #[arg(short, long)]
    key: String,
}

async fn authenticate(
    State(state): State<api::ApiState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    match auth_header {
        Some(auth_header) if auth_header == state.auth => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[debug_handler]
async fn ws_handler(
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        String::from("Unknown browser")
    };
    println!("`{user_agent}` at {addr} connected.");
    // finalize the upgrade process by returning upgrade callback.
    // we can customize the callback by sending additional info such as address.
    ws.on_upgrade(move |socket| handle_socket(socket, addr))
}

/// Actual websocket statemachine (one will be spawned per connection)
async fn handle_socket(mut socket: WebSocket, who: SocketAddr) {
    // send a ping (unsupported by some browsers) just to kick things off and get a response
    if socket
        .send(Message::Ping(Bytes::from_static(&[1, 2, 3])))
        .await
        .is_ok()
    {
        println!("Pinged {who}...");
    } else {
        println!("Could not send ping {who}!");
        // no Error here since the only thing we can do is to close the connection.
        // If we can not send messages, there is no way to salvage the statemachine anyway.
        return;
    }

    // receive single message from a client (we can either receive or send with socket).
    // this will likely be the Pong for our Ping or a hello message from client.
    // waiting for message from a client will block this task, but will not block other client's
    // connections.
    if let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            if process_message(msg, who).is_break() {
                return;
            }
        } else {
            println!("client {who} abruptly disconnected");
            return;
        }
    }

    // Since each client gets individual statemachine, we can pause handling
    // when necessary to wait for some external event (in this case illustrated by sleeping).
    // Waiting for this client to finish getting its greetings does not prevent other clients from
    // connecting to server and receiving their greetings.
    for i in 1..5 {
        if socket
            .send(Message::Text(format!("Hi {i} times!").into()))
            .await
            .is_err()
        {
            println!("client {who} abruptly disconnected");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // By splitting socket we can send and receive at the same time. In this example we will send
    // unsolicited messages to client based on some sort of server's internal event (i.e .timer).
    let (mut sender, mut receiver) = socket.split();

    // Spawn a task that will push several messages to the client (does not matter what client does)
    let mut send_task = tokio::spawn(async move {
        let n_msg = 20;
        for i in 0..n_msg {
            // In case of any websocket error, we exit.
            if sender
                .send(Message::Text(format!("Server message {i} ...").into()))
                .await
                .is_err()
            {
                return i;
            }

            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        println!("Sending close to {who}...");
        if let Err(e) = sender
            .send(Message::Close(Some(CloseFrame {
                code: axum::extract::ws::close_code::NORMAL,
                reason: Utf8Bytes::from_static("Goodbye"),
            })))
            .await
        {
            println!("Could not send Close due to {e}, probably it is ok?");
        }
        n_msg
    });

    // This second task will receive messages from client and print them on server console
    let mut recv_task = tokio::spawn(async move {
        let mut cnt = 0;
        while let Some(Ok(msg)) = receiver.next().await {
            cnt += 1;
            // print message and break if instructed to do so
            if process_message(msg, who).is_break() {
                break;
            }
        }
        cnt
    });

    // If any one of the tasks exit, abort the other.
    tokio::select! {
        rv_a = (&mut send_task) => {
            match rv_a {
                Ok(a) => println!("{a} messages sent to {who}"),
                Err(a) => println!("Error sending messages {a:?}")
            }
            recv_task.abort();
        },
        rv_b = (&mut recv_task) => {
            match rv_b {
                Ok(b) => println!("Received {b} messages"),
                Err(b) => println!("Error receiving messages {b:?}")
            }
            send_task.abort();
        }
    }

    // returning from the handler closes the websocket connection
    println!("Websocket context {who} destroyed");
}

/// helper to print contents of messages to stdout. Has special treatment for Close.
fn process_message(msg: Message, who: SocketAddr) -> ControlFlow<(), ()> {
    match msg {
        Message::Text(t) => {
            println!(">>> {who} sent str: {t:?}");
        }
        Message::Binary(d) => {
            println!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
        }
        Message::Close(c) => {
            if let Some(cf) = c {
                println!(
                    ">>> {} sent close with code {} and reason `{}`",
                    who, cf.code, cf.reason
                );
            } else {
                println!(">>> {who} somehow sent close message without CloseFrame");
            }
            return ControlFlow::Break(());
        }

        Message::Pong(v) => {
            println!(">>> {who} sent pong with {v:?}");
        }
        // You should never need to manually handle Message::Ping, as axum's websocket library
        // will do so for you automagically by replying with Pong and copying the v according to
        // spec. But if you need the contents of the pings you can see them here.
        Message::Ping(v) => {
            println!(">>> {who} sent ping with {v:?}");
        }
    }
    ControlFlow::Continue(())
}

pub async fn init_server(server: ServerArgs) -> Result<(), BitpartError> {
    tracing_subscriber::fmt()
        .with_max_level(server.verbose.log_level_filter().as_trace())
        .init();

    println!("Server is running!");

    let uri = format!("sqlite://{}?mode=rwc", server.database);
    let db = Database::connect(&uri).await?;
    let key_query = format!("PRAGMA key = '{}';", server.key);
    db.execute_unprepared(&key_query).await?;
    db::migration::migrate(&uri).await?;
    // let database = Database::connect(&uri).await?;

    // let runners = db::runner::list(None, None, &db).await?;

    // for id in runners.iter() {
    //     let db = database.clone();
    //     let id = id.clone();
    //     // tokio::spawn(async move {
    //     //     start_channel(&id, &db).await;
    //     // });
    // }

    let state = api::ApiState {
        db,
        auth: server.auth,
    };

    let app = Router::new()
        .route("/api/v1/bots", post(api::post_bot))
        .route(
            "/api/v1/bots/{id}",
            get(api::get_bot).delete(api::delete_bot),
        )
        .route("/api/v1/bots", get(api::list_bots))
        .route("/api/v1/bots/{id}/versions", get(api::get_bot_versions))
        .route(
            "/api/v1/bots/{id}/versions/{id}",
            get(api::get_bot_version).delete(api::delete_bot_version),
        )
        .route(
            "/api/v1/runners",
            post(api::post_runner).get(api::get_runners),
        )
        .route(
            "/api/v1/channels/{id}",
            get(api::get_runner).delete(api::delete_runner),
        )
        // .route("/api/v1/channels/:id/link", post(api::link_device_channel))
        // .route("/api/v1/channels/:id/add", post(api::add_device_channel))
        .route("/api/v1/requests", post(api::post_request))
        .route("/api/v1/ws", any(ws_handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);

    let addr: SocketAddr = server.bind.parse().expect("Unable to parse bind address");

    axum_server::bind(addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}
