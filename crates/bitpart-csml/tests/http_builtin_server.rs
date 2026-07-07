// Integration tests for the http builtin.
// Exercises the async `http_request` leaf against a local tiny_http mock server for
// GET, POST (JSON body), and a TLS endpoint with disable_ssl_verify.

mod support;

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

use bitpart_csml::data::ast::Interval;
use bitpart_csml::data::context::Context;
use bitpart_csml::data::event::Event;
use bitpart_csml::data::primitive::{PrimitiveBoolean, PrimitiveObject, PrimitiveString};
use bitpart_csml::data::Literal;
use bitpart_csml::interpreter::builtins::http_builtin::http_request;

use crate::support::tools::{build_bot, interpret_collecting};

use tiny_http::{Method, Response, Server, SslConfig};

fn header_map(interval: Interval) -> Literal {
    let mut header = HashMap::new();
    header.insert(
        "Content-Type".to_owned(),
        PrimitiveString::get_literal("application/json", interval),
    );
    PrimitiveObject::get_literal(&header, interval)
}

fn build_object(
    url: &str,
    disable_ssl: bool,
    body: Option<serde_json::Value>,
) -> HashMap<String, Literal> {
    let interval = Interval::default();
    let mut object = HashMap::new();
    object.insert(
        "url".to_owned(),
        PrimitiveString::get_literal(url, interval),
    );
    object.insert("header".to_owned(), header_map(interval));
    if disable_ssl {
        object.insert(
            "disable_ssl_verify".to_owned(),
            PrimitiveBoolean::get_literal(true, interval),
        );
    }
    if let Some(body) = body {
        let lit = bitpart_csml::interpreter::json_to_literal(&body, interval, "flow").unwrap();
        object.insert("body".to_owned(), lit);
    }
    object
}

// Spawns an http mock server on an ephemeral port; returns its base url and a join handle.
// The server answers exactly `requests` times then exits, echoing method + body as JSON.
fn spawn_http_server(requests: usize) -> (String, thread::JoinHandle<()>) {
    let server = Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let url = format!("http://{}", addr);

    let handle = thread::spawn(move || {
        for _ in 0..requests {
            let mut request = match server.recv() {
                Ok(r) => r,
                Err(_) => break,
            };
            let method = request.method().to_string();
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let payload = serde_json::json!({ "method": method, "echo": body });
            let data = serde_json::to_string(&payload).unwrap();
            let response = Response::from_string(data).with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );
            let _ = request.respond(response);
        }
    });

    (url, handle)
}

#[tokio::test]
async fn http_get_against_mock_server() {
    let (url, handle) = spawn_http_server(1);
    let object = build_object(&url, false, None);

    let (value, info) = http_request(&object, "get", "flow", Interval::default(), false)
        .await
        .expect("get failed");

    assert_eq!(value.get("method").unwrap(), "GET");
    assert_eq!(info.get("status").unwrap().primitive.to_string(), "200");
    handle.join().unwrap();
}

#[tokio::test]
async fn http_post_json_body_against_mock_server() {
    let (url, handle) = spawn_http_server(1);
    let body = serde_json::json!({ "hello": "world" });
    let object = build_object(&url, false, Some(body));

    let (value, info) = http_request(&object, "post", "flow", Interval::default(), false)
        .await
        .expect("post failed");

    assert_eq!(value.get("method").unwrap(), "POST");
    // server echoes the raw request body back; confirm our JSON body made it across.
    let echo = value.get("echo").unwrap().as_str().unwrap();
    let echoed: serde_json::Value = serde_json::from_str(echo).unwrap();
    assert_eq!(echoed, serde_json::json!({ "hello": "world" }));
    assert_eq!(info.get("status").unwrap().primitive.to_string(), "200");
    handle.join().unwrap();
}

#[tokio::test]
async fn http_status_error_is_mapped() {
    // Server replies 404 -> http_request must surface an Err (>=400 is an error).
    let server = Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let url = format!("http://{}", addr);
    let handle = thread::spawn(move || {
        if let Ok(request) = server.recv() {
            let _ = request.respond(Response::from_string("nope").with_status_code(404));
        }
    });

    let object = build_object(&url, false, None);
    let result = http_request(&object, "get", "flow", Interval::default(), false).await;
    assert!(result.is_err(), "expected 404 to map to Err");
    handle.join().unwrap();
}

#[tokio::test]
async fn http_redirect_limit_is_five() {
    // The client caps redirects at 5, so a server that redirects forever must surface
    // an Err once the limit is exceeded.
    let server = Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let url = format!("http://{}", addr);
    let handle = thread::spawn(move || {
        // Answer enough times to outlast the 5-redirect budget.
        for _ in 0..8 {
            let request = match server.recv() {
                Ok(r) => r,
                Err(_) => break,
            };
            let response = Response::from_string("").with_status_code(302).with_header(
                tiny_http::Header::from_bytes(&b"Location"[..], &b"/next"[..]).unwrap(),
            );
            let _ = request.respond(response);
        }
    });

    let object = build_object(&url, false, None);
    let result = http_request(&object, "get", "flow", Interval::default(), false).await;
    assert!(
        result.is_err(),
        "an endless redirect chain must error once the 5-redirect limit is exceeded"
    );
    drop(handle);
}

// INTERPRET-PATH test: drives a CSML flow `HTTP(_env.url).send()` through interpret() ->
// the async do_exec "send" seam -> http_request leaf, against the same tiny_http mock the
// leaf tests use. The leaf tests call http_request directly; this proves the full async
// dispatch chain (interpret -> execute_step -> do_exec special-case -> send_http) is wired.
// host:port injected hermetically via bot env (`_env`), no templating of the CSML string.
#[test]
fn http_send_through_interpret() {
    let (url, handle) = spawn_http_server(1);

    let flow = r#"
start:
    do response = HTTP(_env.url).send()
    say response.method
    goto end
"#;
    let bot = build_bot(flow, Some(serde_json::json!({ "url": url })));
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

    let (msg_data, messages) = interpret_collecting(bot, context, event);

    // Streamed MSGs must include the GET method echoed by the mock (proves the request
    // actually went out through the async send seam, not a stubbed value).
    let said: Vec<String> = msg_data
        .messages
        .iter()
        .filter(|m| m.content_type == "text")
        .map(|m| m.content["text"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        said.contains(&"GET".to_string()),
        "expected the mock-echoed GET method in say output, got: {:?}",
        said
    );
    // The streaming harness must have observed the same message(s) concurrently.
    assert!(!messages.is_empty(), "streaming consumer collected no MSGs");
    handle.join().unwrap();
}

#[tokio::test]
async fn https_with_disable_ssl_verify() {
    // Self-signed cert => reqwest would reject it without danger_accept_invalid_certs.
    // disable_ssl_verify=true must let the call through, proving NoVerifier parity.
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_pem = cert.serialize_pem().unwrap().into_bytes();
    let key_pem = cert.serialize_private_key_pem().into_bytes();

    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let server = Server::https(
            "127.0.0.1:0",
            SslConfig {
                certificate: cert_pem,
                private_key: key_pem,
            },
        )
        .unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        tx.send(port).unwrap();

        if let Ok(request) = server.recv() {
            assert_eq!(request.method(), &Method::Get);
            let payload = serde_json::json!({ "tls": "ok" });
            let response = Response::from_string(serde_json::to_string(&payload).unwrap());
            let _ = request.respond(response);
        }
    });

    let port = rx.recv().unwrap();
    let url = format!("https://localhost:{}", port);

    // The per-call `disable_ssl_verify` flag now works standalone, with no
    // DISABLE_SSL_VERIFY env var present (the env var is an additional global override).
    std::env::remove_var("DISABLE_SSL_VERIFY");

    // Sanity: without the per-call flag the self-signed cert should be rejected.
    let object_secure = build_object(&url, false, None);
    let secure_result =
        http_request(&object_secure, "get", "flow", Interval::default(), false).await;
    assert!(
        secure_result.is_err(),
        "self-signed cert should fail without disable_ssl_verify"
    );

    let object = build_object(&url, true, None);
    let (value, info) = http_request(&object, "get", "flow", Interval::default(), false)
        .await
        .expect("https with disable_ssl_verify failed");

    assert_eq!(value.get("tls").unwrap(), "ok");
    assert_eq!(info.get("status").unwrap().primitive.to_string(), "200");
    handle.join().unwrap();
}
