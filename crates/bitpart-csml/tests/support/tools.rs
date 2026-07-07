use bitpart_csml::data::csml_bot::CsmlBot;
use bitpart_csml::data::csml_flow::CsmlFlow;
use bitpart_csml::data::event::Event;
use bitpart_csml::data::message_data::MessageData;
use bitpart_csml::data::msg::MSG;
use bitpart_csml::data::Context;
use bitpart_csml::{interpret, load_components};
use serde_json::{json, map::Map, Value};

use std::fs::File;
use std::io::prelude::*;
use std::sync::mpsc;

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

pub fn read_file(file_path: String) -> Result<String, ::std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut contents = String::new();

    file.read_to_string(&mut contents)?;

    Ok(contents)
}

#[allow(dead_code)]
pub fn format_message(event: Event, context: Context, filepath: &str) -> MessageData {
    let content = read_file(filepath.to_string()).unwrap();

    let flow = CsmlFlow::new("id", "flow", &content, Vec::default());
    let native_component = load_components().unwrap();

    let bot = CsmlBot::new(
        "id",
        "bot",
        None,
        vec![flow],
        Some(native_component),
        None,
        "flow",
        None,
        None,
        None,
        None,
        None,
    );

    // interpret is now async; tests stay sync by blocking on a local runtime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(interpret(bot, context, event, None))
}

// Builds a single-flow CsmlBot from inline CSML `content`, with an optional `env`
// object exposed to the flow as `_env` (used to inject hermetic mock host:port into
// interpret-path tests without templating the CSML string).
//
// Sibling to `format_message`, not a replacement: `format_message` reads a flow from
// disk and returns only the MessageData. The interpret-path tests need (a) inline CSML,
// (b) an injected env, and (c) the streamed MSGs — none of which format_message exposes,
// and widening its signature would churn the ~100 call sites that use it.
#[allow(dead_code)]
pub fn build_bot(content: &str, env: Option<Value>) -> CsmlBot {
    let flow = CsmlFlow::new("id", "flow", content, Vec::default());
    let native_component = load_components().unwrap();

    CsmlBot::new(
        "id",
        "bot",
        None,
        vec![flow],
        Some(native_component),
        None,
        "flow",
        None,
        None,
        env,
        None,
        None,
    )
}

// Runs `interpret` to completion AND collects the MSGs streamed over the std::sync::mpsc
// channel into a Vec for assertions. The receiver is drained on a dedicated OS thread
// concurrently with interpret (streaming), not collected after the future resolves.
// interpret runs as a spawned tokio task; when it (and thus its sender clone) is dropped,
// the receiver loop ends and the consumer thread joins.
#[allow(dead_code)]
pub fn interpret_collecting(
    bot: CsmlBot,
    context: Context,
    event: Event,
) -> (MessageData, Vec<MSG>) {
    let (sender, receiver) = mpsc::channel::<MSG>();

    // CONSUMER: drain incrementally on its own thread, per-message as interpret produces.
    let consumer = std::thread::spawn(move || {
        let mut collected = Vec::new();
        for received in receiver.iter() {
            collected.push(received);
        }
        collected
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    // PRODUCER: spawn interpret as a task so it runs concurrently with the consumer.
    let msg_data = runtime.block_on(async move {
        let handle =
            tokio::spawn(async move { interpret(bot, context, event, Some(sender)).await });
        handle.await.unwrap()
    });

    let messages = consumer.join().unwrap();
    (msg_data, messages)
}

#[allow(dead_code)]
pub fn message_to_json_value(result: MessageData) -> Value {
    let mut message: Map<String, Value> = Map::new();
    let mut vec = vec![];
    let mut memories = vec![];

    for msg in result.messages.iter() {
        vec.push(msg.to_owned().message_to_json());
    }

    if let Some(mem) = result.memories {
        for elem in mem.iter() {
            let mut map = Map::new();
            map.insert("key".to_owned(), json!(elem.key.to_owned()));
            map.insert("value".to_owned(), elem.value.to_owned());
            memories.push(json!(map));
        }
    }

    message.insert("memories".to_owned(), Value::Array(memories));
    message.insert("messages".to_owned(), Value::Array(vec));

    Value::Object(message)
}
