// Behavior tests for under-tested builtins, driven through interpret() via the
// MSG-collecting harness. Each asserts a known input -> known output.
//
// The crypto hash + hmac cases are deliberately byte-exact: the hash digests and the
// RSA signature were captured with /usr/bin/openssl (see comments per case).

mod support;

use std::collections::HashMap;

use bitpart_csml::data::context::Context;
use bitpart_csml::data::event::Event;
use serde_json::Value;

use crate::support::tools::{build_bot, interpret_collecting};

// Runs a single-flow CSML string through interpret and returns the `say`d text messages
// in order. `env` is exposed to the flow as `_env`.
fn run_says(flow: &str, env: Option<Value>) -> Vec<String> {
    let bot = build_bot(flow, env);
    let context = Context::new(
        HashMap::new(),
        HashMap::new(),
        None,
        None,
        "start",
        "flow",
        None,
    );
    let event = Event::new("payload", "", serde_json::json!({}));
    let (msg_data, _streamed) = interpret_collecting(bot, context, event);

    // Surface any interpreter error as a panic so a broken flow can't false-green.
    if let Some(err) = msg_data.messages.iter().find(|m| m.content_type == "error") {
        panic!("flow produced an error message: {:?}", err);
    }

    msg_data
        .messages
        .iter()
        .filter(|m| m.content_type == "text")
        .map(|m| m.content["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

////////////////////////////////////////////////////////////////////////////////
// CRYPTO HASH
// Known input "The quick brown fox jumps over the lazy dog".
// Expected digests verified with: printf '%s' "<input>" | openssl dgst -<algo>
////////////////////////////////////////////////////////////////////////////////

const HASH_INPUT: &str = "The quick brown fox jumps over the lazy dog";

#[test]
fn crypto_hash_md5_parity() {
    let flow = r#"
start:
    say Crypto(_env.input).create_hash("md5").digest("hex")
    goto end
"#;
    let says = run_says(flow, Some(serde_json::json!({ "input": HASH_INPUT })));
    // openssl md5
    assert_eq!(says, vec!["9e107d9d372bb6826bd81d3542a419d6".to_string()]);
}

#[test]
fn crypto_hash_sha1_parity() {
    let flow = r#"
start:
    say Crypto(_env.input).create_hash("sha1").digest("hex")
    goto end
"#;
    let says = run_says(flow, Some(serde_json::json!({ "input": HASH_INPUT })));
    // openssl sha1
    assert_eq!(
        says,
        vec!["2fd4e1c67a2d28fced849ee1bb76e7391b93eb12".to_string()]
    );
}

#[test]
fn crypto_hash_sha256_parity() {
    let flow = r#"
start:
    say Crypto(_env.input).create_hash("sha256").digest("hex")
    goto end
"#;
    let says = run_says(flow, Some(serde_json::json!({ "input": HASH_INPUT })));
    // openssl sha256
    assert_eq!(
        says,
        vec!["d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592".to_string()]
    );
}

#[test]
fn crypto_hash_sha384_parity() {
    let flow = r#"
start:
    say Crypto(_env.input).create_hash("sha384").digest("hex")
    goto end
"#;
    let says = run_says(flow, Some(serde_json::json!({ "input": HASH_INPUT })));
    // openssl sha384
    assert_eq!(
        says,
        vec!["ca737f1014a48f4c0b6dd43cb177b0afd9e5169367544c494011e3317dbf9a509cb1e5dc1e85a941bbee3d7f2afbc9b1".to_string()]
    );
}

#[test]
fn crypto_hash_sha512_parity() {
    let flow = r#"
start:
    say Crypto(_env.input).create_hash("sha512").digest("hex")
    goto end
"#;
    let says = run_says(flow, Some(serde_json::json!({ "input": HASH_INPUT })));
    // openssl sha512
    assert_eq!(
        says,
        vec!["07e547d9586f6a73f73fbac0435ed76951218fb7d0c8d788a309d785436bbb642e93a252a954f23912547d1e8a3b5ed6e1bfd7097821233fa0538f3db854fee6".to_string()]
    );
}

#[test]
fn crypto_hash_digest_base64_parity() {
    // Same sha256 digest, base64-encoded via the "base64" digest path (boring::base64).
    let flow = r#"
start:
    say Crypto(_env.input).create_hash("sha256").digest("base64")
    goto end
"#;
    let says = run_says(flow, Some(serde_json::json!({ "input": HASH_INPUT })));
    // base64 of the sha256 digest bytes (openssl dgst -sha256 -binary | base64).
    assert_eq!(
        says,
        vec!["16j7swfXgJRpypq8sAguT41WUeRtPNt2LQLQvzfJ5ZI=".to_string()]
    );
}

////////////////////////////////////////////////////////////////////////////////
// CRYPTO HMAC / SIGN
// Fixed RSA-2048 key fixture: tests/support/test_rsa_key.pem.
// create_hmac(algo, pem_key) builds a boring Signer over the PEM private key; for an RSA
// key + sha256 this is an RSA PKCS#1 v1.5 signature.
// Expected signature verified with:
//   printf 'hello world' | openssl dgst -sha256 -sign tests/support/test_rsa_key.pem -binary | xxd -p
////////////////////////////////////////////////////////////////////////////////

#[test]
fn crypto_sign_rsa_sha256_parity() {
    let pem = include_str!("support/test_rsa_key.pem");
    let flow = r#"
start:
    say Crypto("hello world").create_hmac("sha256", _env.key).digest("hex")
    goto end
"#;
    let says = run_says(flow, Some(serde_json::json!({ "key": pem })));
    // RSA PKCS#1 v1.5 over sha256("hello world") with the committed key.
    assert_eq!(
        says,
        vec!["343abe9a6968da4efc1bdc70470d9bba2dbdff921c3002758ab7e5078e3938579ea492bf173971a2c6a18b100fc34ff8e30b2a409d8bc7ef0ce4fb7e248af1db793f24c15f08041433d56160b3b6b20846a54262a9fba6e4fc8f7086f97189025b40541e4482d18c6e92c7ff56bd35b3e9052768f423f120edb2bdc4296bf097b556da2c7a4de3e96d11451e967a77d6901dbb72985a8e8889611fdfcaac2f3cfec2ba61c720e450b1850a85010ab339083d2a72d20d1950eb78374123f3dfd4ef23d5cde7ef4b54e5ec9d25a5f4bf5200d1d14ddd313a895b590ce9388a2663f7ac2b10a12781f06420d8a45d7923fcfce7a592ac6dccb61abb0fc1ee8246a2".to_string()]
    );
}

////////////////////////////////////////////////////////////////////////////////
// BASE64 (pure-Rust `base64` crate) — known input->output behavior.
////////////////////////////////////////////////////////////////////////////////

#[test]
fn base64_encode_decode_known() {
    let flow = r#"
start:
    say Base64("Hello World").encode()
    say Base64("SGVsbG8gV29ybGQ=").decode()
    goto end
"#;
    let says = run_says(flow, None);
    assert_eq!(
        says,
        vec!["SGVsbG8gV29ybGQ=".to_string(), "Hello World".to_string()]
    );
}

////////////////////////////////////////////////////////////////////////////////
// HEX (pure-Rust `hex` crate) — known input->output behavior.
////////////////////////////////////////////////////////////////////////////////

#[test]
fn hex_encode_decode_known() {
    let flow = r#"
start:
    say Hex("Hello World").encode()
    say Hex("48656c6c6f20576f726c64").decode()
    goto end
"#;
    let says = run_says(flow, None);
    assert_eq!(
        says,
        vec![
            "48656c6c6f20576f726c64".to_string(),
            "Hello World".to_string()
        ]
    );
}

////////////////////////////////////////////////////////////////////////////////
// JWT — encode (sign) then decode round-trip with a known HS256 secret.
////////////////////////////////////////////////////////////////////////////////

#[test]
fn jwt_sign_decode_round_trip() {
    // Sign a claims object, then decode it back and read a claim. Asserts the round-trip
    // recovers the original value (HS256, fixed secret). Token string itself is not pinned
    // (jsonwebtoken's compact serialization is stable but the claim recovery is the contract).
    let flow = r#"
start:
    do token = JWT({"user": "alice", "n": 42, "exp": 9999999999}).sign("HS256", "topsecret")
    do claims = JWT(token).decode("HS256", "topsecret")
    say claims.payload.user
    say claims.payload.n
    goto end
"#;
    let says = run_says(flow, None);
    assert_eq!(says, vec!["alice".to_string(), "42".to_string()]);
}

////////////////////////////////////////////////////////////////////////////////
// TIME / FORMAT — deterministic known input->output via a fixed date.
////////////////////////////////////////////////////////////////////////////////

#[test]
fn time_at_format_known() {
    // Time().at(y, m, d) sets a fixed date; format("%Y") must yield the year deterministically
    // (no dependence on wall-clock => hermetic).
    let flow = r#"
start:
    do t = Time()
    do t.at(2014, 10, 20)
    say t.format("%Y")
    goto end
"#;
    let says = run_says(flow, None);
    assert_eq!(says, vec!["2014".to_string()]);
}

#[test]
fn time_parse_format_known() {
    // parse a fixed ISO date and reformat a component.
    let flow = r#"
start:
    do t = Time().parse("1983-08-13")
    say t.format("%Y-%m-%d")
    goto end
"#;
    let says = run_says(flow, None);
    assert_eq!(says, vec!["1983-08-13".to_string()]);
}
