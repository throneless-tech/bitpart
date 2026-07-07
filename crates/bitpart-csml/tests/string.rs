mod support;

use bitpart_csml::data::context::Context;
use bitpart_csml::data::event::Event;
use std::collections::HashMap;

use crate::support::tools::format_message;
use crate::support::tools::message_to_json_value;

use serde_json::Value;

#[test]
fn string_step_0() {
    let data = r#"{
        "memories":[
            {"key":"s", "value":"Hello "},
            {"key":"s", "value":"Hello World"}
        ],
        "messages":[
            {"content":{"text": "Hello World"}, "content_type":"text"},
            {"content":{"text": "HELLO WORLD"}, "content_type":"text"},
            {"content":{"text": "hello world"}, "content_type":"text"}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_0",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_1() {
    let data = r#"{
        "memories":[
            {"key":"s", "value":"Hello"}
        ],
        "messages":[
            {"content":{"text": "true"}, "content_type":"text"},
            {"content":{"text": "true"}, "content_type":"text"},
            {"content":{"text": "true"}, "content_type":"text"}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_1",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_2() {
    let data = r#"{
        "memories":[
            {"key":"s", "value":"Hello"}
        ],
        "messages":[
            {"content":{"text": "true"}, "content_type":"text"},
            {"content":{"text": "true"}, "content_type":"text"},
            {"content":{"text": "false"}, "content_type":"text"},
            {"content":{"text": "false"}, "content_type":"text"},
            {"content":{"text": "false"}, "content_type":"text"}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_2",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_3() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content":{"text": "true"}, "content_type":"text"}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_3",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_4() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "[\"Hello\"]"}},
            {"content_type":"text", "content":{"text": "[\"\",\"ello\"]"}},
            {"content_type":"text", "content":{"text": "[\"He\",\"\",\"o\"]"}},
            {"content_type":"text", "content":{"text": "[\"He\",\"o\"]"}},
            {"content_type":"text", "content":{"text": "[\"Hell\",\"\"]"}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_4",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_5() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "Hello World"}},
            {"content_type":"text", "content":{"text": "Hello"}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_5",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_6() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "Hello World"}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_6",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_7() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "😆"}},
            {"content_type":"text", "content":{"text": "H"}},
            {"content_type":"text", "content":{"text": "e"}},
            {"content_type":"text", "content":{"text": "l"}},
            {"content_type":"text", "content":{"text": "l"}},
            {"content_type":"text", "content":{"text": "o"}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_7",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_8() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "😆"}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_8",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_9() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "😆"}},
            {"content_type":"text", "content":{"text": "😆Hello World 😆"}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_9",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_10() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "test Hello World 😆"}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_10",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_11() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "false"}},
            {"content_type":"text", "content":{"text": "Hello World"}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_11",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_12_slice() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "Hel"}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_12_slice",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_13_to_string() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "\"4\""}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_13_to_string",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_14_to_string() {
    let data = r#"{
        "memories":[],
        "messages":[
            {"content_type":"text", "content":{"text": "😆\"2\""}}
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_14_to_string",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_15_xml() {
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_15_xml",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let text = v1["messages"][0]["content"]["text"]
        .as_str()
        .expect("message text");

    // to_xml() serializes an unordered object, so the two child elements may appear
    // in either order. Both orderings are valid.
    let valid = [
        "<Item><name>Banana</name><source>Store</source></Item>",
        "<Item><source>Store</source><name>Banana</name></Item>",
    ];
    assert!(valid.contains(&text), "unexpected xml output: {text}");
}

#[test]
fn string_step_16_yaml() {
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_16_yaml",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let text = v1["messages"][0]["content"]["text"]
        .as_str()
        .expect("message text");

    // to_yaml() serializes an unordered object, so key order is not guaranteed.
    // Parse both sides back to compare structurally rather than by byte order.
    let produced: serde_yaml::Value = serde_yaml::from_str(text).expect("valid yaml");
    let expected: serde_yaml::Value =
        serde_yaml::from_str("---\nx: 1.0\ny: 2.0\n").expect("valid yaml");
    assert_eq!(produced, expected);
}

#[test]
fn string_step_17_uri() {
    let data = r#"{
        "memories":[],
        "messages":[
            {
                "content_type":"text", "content": {"text": "https://mozilla.org/?key=%D1%8B&key2=value2#fragid1=1,4,%D1%8B,6"}
            },
            {
                "content_type":"text", "content": {"text": "https://mozilla.org/?key=ы&key2=value2#fragid1=1,4,ы,6"}
            }
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_17_uri",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}

#[test]
fn string_step_18_html_escape() {
    let data = r#"{
        "memories":[],
        "messages":[
            {
                "content_type":"text", "content": {"text": "&lt;a&gt;&lt;b&gt;42&lt;/b&gt;&lt;/a&gt;"}
            },
            {
                "content_type":"text", "content": {"text": "<a><b>42</b></a>"}
            }
        ]}"#;
    let msg = format_message(
        Event::new("payload", "", serde_json::json!({})),
        Context::new(
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            "step_18_html_escape",
            "flow",
            None,
        ),
        "CSML/basic_test/stdlib/string.csml",
    );

    let v1: Value = message_to_json_value(msg);
    let v2: Value = serde_json::from_str(data).unwrap();

    assert_eq!(v1, v2)
}
