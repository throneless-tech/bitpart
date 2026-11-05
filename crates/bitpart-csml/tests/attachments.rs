mod support;

use std::collections::HashMap;

use bitpart_csml::data::context::Context;
use bitpart_csml::data::event::Event;
use bitpart_csml::data::msg::MSG;
use serde_json::{Value, json};

use crate::support::tools::{build_bot, interpret_collecting};

fn run(flow: &str, content: Value) -> Vec<MSG> {
    let bot = build_bot(flow, None);
    let context = Context::new(
        HashMap::new(),
        HashMap::new(),
        None,
        None,
        "start",
        "flow",
        None,
    );
    let text = content["text"].as_str().unwrap_or_default().to_owned();
    let event = Event::new("text", &text, content);
    let (msg_data, streamed) = interpret_collecting(bot, context, event);

    if let Some(err) = msg_data.messages.iter().find(|m| m.content_type == "error") {
        panic!("flow produced an error message: {:?}", err);
    }

    streamed
}

fn relayed(messages: &[MSG]) -> Vec<(&'static str, Value)> {
    messages
        .iter()
        .filter_map(|m| match m {
            MSG::Message(msg) => Some(("say", msg.content.clone())),
            MSG::Shout(msg) => Some(("shout", msg.content.clone())),
            MSG::Whisper(msg) => Some(("whisper", msg.content.clone())),
            _ => None,
        })
        .collect()
}

fn attachment(name: &str) -> Value {
    json!({ "_ref": "cmVm", "content_type": "image/jpeg", "file_name": name, "size": 3 })
}

#[test]
fn text_with_attachments_keeps_them_on_every_relay() {
    let flow = r#"
start:
    say Text(event, attachments=event.attachments)
    shout Text(event, attachments=event.attachments)
    whisper Text("tip: {{event}}", attachments=event.attachments)
    goto end
"#;
    let messages = run(
        flow,
        json!({ "text": "hello", "attachments": [attachment("a.jpg")] }),
    );

    let relayed = relayed(&messages);
    assert_eq!(relayed.len(), 3);
    for (kind, content) in &relayed {
        assert_eq!(
            content["attachments"],
            json!([attachment("a.jpg")]),
            "{kind}"
        );
    }
    assert_eq!(relayed[0].1["text"], "hello");
    assert_eq!(relayed[2].1["text"], "tip: hello");
}

#[test]
fn text_with_no_attachments_still_sends_the_text() {
    let flow = r#"
start:
    whisper Text("tip: {{event}}", attachments=event.attachments)
    goto end
"#;
    let messages = run(flow, json!({ "text": "hello", "attachments": [] }));

    let relayed = relayed(&messages);
    assert_eq!(relayed.len(), 1);
    assert_eq!(relayed[0].1["text"], "tip: hello");
    assert_eq!(relayed[0].1["attachments"], json!([]));
}

#[test]
fn extra_named_flags_reach_the_message_content() {
    let flow = r#"
start:
    whisper Text("tip", attachments=event.attachments, strip_file_names=true)
    goto end
"#;
    let messages = run(
        flow,
        json!({ "text": "", "attachments": [attachment("c.pdf")] }),
    );

    let relayed = relayed(&messages);
    assert_eq!(relayed.len(), 1);
    assert_eq!(relayed[0].1["strip_file_names"], true);
    assert_eq!(relayed[0].1["attachments"], json!([attachment("c.pdf")]));
}

#[test]
fn remembered_attachments_survive_until_the_relay() {
    let flow = r#"
start:
    remember saved = event.attachments
    goto relay

relay:
    whisper Text("later", attachments=saved)
    goto end
"#;
    let messages = run(
        flow,
        json!({ "text": "", "attachments": [attachment("b.png")] }),
    );

    let relayed = relayed(&messages);
    assert_eq!(relayed.len(), 1);
    assert_eq!(relayed[0].1["attachments"], json!([attachment("b.png")]));
}
