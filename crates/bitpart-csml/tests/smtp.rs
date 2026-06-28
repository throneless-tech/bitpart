// Integration tests for the async SMTP send builtin.
// Exercises the real production path: `tools_smtp::format_email` (builds the
// lettre::Message) + `tools_smtp::get_mailer` (builds an
// AsyncSmtpTransport<Tokio1Executor>) + `mailer.send(email).await`.
//
// Hermetic: a minimal in-process tokio TcpListener speaks just enough SMTP to
// accept (or reject) a single lettre send. No real network / DNS / SMTP provider.
// tls=false (builder_dangerous) so no cert setup is needed.

mod support;

use std::collections::HashMap;

use bitpart_csml::data::ast::{Flow, FlowType, Interval};
use bitpart_csml::data::context::Context;
use bitpart_csml::data::literal::ContentType;
use bitpart_csml::data::memories::MemoryType;
use bitpart_csml::data::message_data::MessageData;
use bitpart_csml::data::primitive::tools_smtp;
use bitpart_csml::data::primitive::{
    PrimitiveArray, PrimitiveBoolean, PrimitiveInt, PrimitiveObject, PrimitiveString, PrimitiveType,
};
use bitpart_csml::data::{Data, Event, Literal};

use crate::support::tools::{build_bot, interpret_collecting};

use lettre::AsyncTransport;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

////////////////////////////////////////////////////////////////////////////////
// MOCK SMTP SERVER
////////////////////////////////////////////////////////////////////////////////

// Handles exactly one SMTP session over an already-accepted stream.
// `reject_rcpt`: when true, replies 550 to RCPT TO (failure-path test).
async fn handle_smtp_session(stream: TcpStream, reject_rcpt: bool) {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // Greeting.
    let _ = write_half.write_all(b"220 mock.smtp.local ESMTP\r\n").await;

    let mut line = String::new();
    loop {
        line.clear();
        let n = match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let _ = n;
        let upper = line.trim_end().to_uppercase();

        if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            // Advertise capabilities, terminating with a "250 " line.
            let _ = write_half
                .write_all(b"250-mock.smtp.local\r\n250 AUTH PLAIN LOGIN\r\n")
                .await;
        } else if upper.starts_with("MAIL FROM") {
            let _ = write_half.write_all(b"250 OK\r\n").await;
        } else if upper.starts_with("RCPT TO") {
            if reject_rcpt {
                let _ = write_half.write_all(b"550 No such recipient\r\n").await;
            } else {
                let _ = write_half.write_all(b"250 OK\r\n").await;
            }
        } else if upper.starts_with("DATA") {
            let _ = write_half
                .write_all(b"354 End data with <CR><LF>.<CR><LF>\r\n")
                .await;
            // Read body until a line containing only ".".
            loop {
                let mut body_line = String::new();
                match reader.read_line(&mut body_line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if body_line.trim_end() == "." {
                    break;
                }
            }
            let _ = write_half.write_all(b"250 OK: queued\r\n").await;
        } else if upper.starts_with("QUIT") {
            let _ = write_half.write_all(b"221 Bye\r\n").await;
            break;
        } else if upper.starts_with("RSET") || upper.starts_with("NOOP") {
            let _ = write_half.write_all(b"250 OK\r\n").await;
        } else if upper.starts_with("AUTH") {
            // We don't set credentials that force AUTH in these tests, but answer
            // defensively if a client tries it.
            let _ = write_half
                .write_all(b"235 Authentication successful\r\n")
                .await;
        } else {
            let _ = write_half.write_all(b"250 OK\r\n").await;
        }
    }
    let _ = write_half.flush().await;
}

// Binds an ephemeral port and serves exactly one SMTP session, then returns.
// Returns (host, port) plus the spawned task handle.
async fn spawn_mock_smtp(reject_rcpt: bool) -> (String, u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            handle_smtp_session(stream, reject_rcpt).await;
        }
    });
    (addr.ip().to_string(), addr.port(), handle)
}

////////////////////////////////////////////////////////////////////////////////
// TEST HELPERS — minimal Data + smtp object
////////////////////////////////////////////////////////////////////////////////

fn empty_flow() -> Flow {
    Flow {
        flow_instructions: HashMap::new(),
        flow_type: FlowType::Normal,
        constants: HashMap::new(),
    }
}

// Builds an smtp-object value map (host:port, tls=false) the way the `Smtp(...)`
// builtin + `.tls(false)` chain would, pointing at the mock.
fn smtp_object(host: &str, port: u16) -> HashMap<String, Literal> {
    let interval = Interval::default();
    let mut object = HashMap::new();
    object.insert(
        "smtp_server".to_owned(),
        PrimitiveString::get_literal(host, interval),
    );
    object.insert(
        "port".to_owned(),
        PrimitiveInt::get_literal(port as i64, interval),
    );
    object.insert(
        "tls".to_owned(),
        PrimitiveBoolean::get_literal(false, interval),
    );
    object.insert(
        "username".to_owned(),
        PrimitiveString::get_literal("user", interval),
    );
    object.insert(
        "password".to_owned(),
        PrimitiveString::get_literal("pass", interval),
    );
    object
}

// Builds the csml email value map consumed by `format_email`.
fn email_object() -> HashMap<String, Literal> {
    let interval = Interval::default();
    let mut email = HashMap::new();
    email.insert(
        "from".to_owned(),
        PrimitiveString::get_literal("Sender <sender@example.com>", interval),
    );
    email.insert(
        "to".to_owned(),
        PrimitiveString::get_literal("Recipient <recipient@example.com>", interval),
    );
    email.insert(
        "subject".to_owned(),
        PrimitiveString::get_literal("Hello", interval),
    );
    email.insert(
        "text".to_owned(),
        PrimitiveString::get_literal("This is a test email.", interval),
    );
    email
}

// Runs the production format_email + get_mailer + async send against the mock and
// returns the lettre send result. `data`/`context` are constructed minimally; the
// smtp tools only read `data.context.flow` (for error positions).
async fn run_send(reject_rcpt: bool) -> Result<(), String> {
    let (host, port, handle) = spawn_mock_smtp(reject_rcpt).await;

    let flow = empty_flow();
    let flows = HashMap::new();
    let extern_flows = HashMap::new();
    let mut context = Context::new(
        HashMap::new(),
        HashMap::new(),
        None,
        None,
        "start",
        "flow",
        None,
    );
    let event = Event::default();
    let env = PrimitiveObject::get_literal(&HashMap::new(), Interval::default());
    let mut step_count: usize = 0;
    let custom_component = serde_json::Map::new();
    let native_component = serde_json::Map::new();

    let data = Data::new(
        &flows,
        &extern_flows,
        &flow,
        "flow".to_owned(),
        &mut context,
        &event,
        &env,
        vec![],
        0,
        &mut step_count,
        100,
        HashMap::new(),
        None,
        &custom_component,
        &native_component,
    );

    let interval = Interval::default();
    let mut object = smtp_object(&host, port);
    let email_map = email_object();

    // Built before the await: owned Message + owned transport.
    let email = tools_smtp::format_email(&email_map, &data, interval)
        .map_err(|e| format!("format_email failed: {:?}", e))?;
    let mailer = tools_smtp::get_mailer(&mut object, &data, interval)
        .map_err(|e| format!("get_mailer failed: {:?}", e))?;

    let result = mailer.send(email).await.map_err(|e| format!("{:?}", e));

    // lettre's AsyncSmtpTransport pools connections by default and does NOT send
    // QUIT after a successful send, so the mock's session would block on read_line
    // until an idle timeout (~120s). Dropping the mailer closes the pooled
    // connection -> the mock reads EOF and its session task returns immediately.
    drop(mailer);
    let _ = handle.await;
    result.map(|_| ())
}

////////////////////////////////////////////////////////////////////////////////
// TESTS
////////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn smtp_async_send_success() {
    let result = run_send(false).await;
    assert!(
        result.is_ok(),
        "expected successful async smtp send, got: {:?}",
        result
    );
}

// Drives the REAL dispatch seam: build an smtp-typed Literal and call do_exec with
// "set_auth_mechanism" (a sync FUNCTIONS_SMTP method, restored in review #19). Proves
// the phf wiring is live and that the method stores the mechanisms on the object.
#[tokio::test]
async fn smtp_set_auth_mechanism_stores_mechanisms() {
    let interval = Interval::default();
    let flow = empty_flow();
    let flows = HashMap::new();
    let extern_flows = HashMap::new();
    let mut context = Context::new(
        HashMap::new(),
        HashMap::new(),
        None,
        None,
        "start",
        "flow",
        None,
    );
    let event = Event::default();
    let env = PrimitiveObject::get_literal(&HashMap::new(), interval);
    let mut step_count: usize = 0;
    let custom_component = serde_json::Map::new();
    let native_component = serde_json::Map::new();
    let mut data = Data::new(
        &flows,
        &extern_flows,
        &flow,
        "flow".to_owned(),
        &mut context,
        &event,
        &env,
        vec![],
        0,
        &mut step_count,
        100,
        HashMap::new(),
        None,
        &custom_component,
        &native_component,
    );
    let mut msg_data = MessageData::default();
    let sender = None;

    // An smtp-typed object literal (as Smtp("...") would produce).
    let mut obj = HashMap::new();
    obj.insert(
        "smtp_server".to_owned(),
        PrimitiveString::get_literal("smtp.example.com", interval),
    );
    let mut smtp_lit = PrimitiveObject::get_literal(&obj, interval);
    smtp_lit.set_content_type("smtp");

    // args: arg0 = ["PLAIN", "XOAUTH2"]
    let mechanisms = vec![
        PrimitiveString::get_literal("PLAIN", interval),
        PrimitiveString::get_literal("XOAUTH2", interval),
    ];
    let mut args = HashMap::new();
    args.insert(
        "arg0".to_owned(),
        PrimitiveArray::get_literal(&mechanisms, interval),
    );

    let result = smtp_lit
        .primitive
        .do_exec(
            "set_auth_mechanism",
            &args,
            &MemoryType::Use,
            &None,
            interval,
            &ContentType::Smtp,
            &mut data,
            &mut msg_data,
            &sender,
        )
        .await;

    let (lit, _right) = result.expect("set_auth_mechanism should dispatch via FUNCTIONS_SMTP");
    assert_eq!(lit.content_type, "smtp", "result stays an smtp object");
    let map = Literal::get_value::<HashMap<String, Literal>>(
        &lit.primitive,
        "flow",
        interval,
        "expected smtp object map".to_owned(),
    )
    .expect("smtp result is an object map");
    let auth = map
        .get("auth_mechanisms")
        .expect("auth_mechanisms stored on the object");
    assert_eq!(auth.primitive.get_type(), PrimitiveType::PrimitiveObject);
    let auth_map = Literal::get_value::<HashMap<String, Literal>>(
        &auth.primitive,
        "flow",
        interval,
        "auth_mechanisms is a map".to_owned(),
    )
    .expect("auth_mechanisms is an object map");
    assert!(auth_map.contains_key("PLAIN"), "PLAIN mechanism stored");
    assert!(auth_map.contains_key("XOAUTH2"), "XOAUTH2 mechanism stored");
}

// INTERPRET-PATH test (deferred #18 interpret-path coverage): drives a CSML flow
// `Smtp(_env.host).port(_env.port).tls(false).send({...}).send(email)` through interpret()
// -> the async do_exec "send" seam -> smtp_send leaf, against the same in-process mock the
// leaf tests use. host:port injected hermetically via bot env (`_env`), no CSML templating.
//
// The mock is async (tokio TcpListener); interpret_collecting owns its OWN blocking runtime
// (support/tools.rs). To keep the two runtimes isolated, the mock runs on a dedicated OS
// thread with its own current-thread runtime and hands its ephemeral port back over a
// std::sync::mpsc channel before interpret runs.
#[test]
fn smtp_send_through_interpret() {
    use std::sync::mpsc;

    let (port_tx, port_rx) = mpsc::channel::<u16>();
    let mock = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            port_tx.send(listener.local_addr().unwrap().port()).unwrap();
            if let Ok((stream, _)) = listener.accept().await {
                handle_smtp_session(stream, false).await;
            }
        });
    });

    let port = port_rx.recv().unwrap();

    let flow = r#"
start:
    do email = {
        "from": "Sender <sender@example.com>",
        "to": "Recipient <recipient@example.com>",
        "subject": "Hello",
        "text": "This is a test email."
    }
    do sent = SMTP(_env.host).auth("user", "pass").port(_env.port).tls(false).send(email)
    say sent
    goto end
"#;
    let bot = build_bot(
        flow,
        Some(serde_json::json!({ "host": "127.0.0.1", "port": port })),
    );
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

    let (msg_data, _messages) = interpret_collecting(bot, context, event);

    // smtp_send returns Boolean(true) on a successful send; the flow `say`s it. An error
    // would surface as a content_type=="error" message instead.
    let errors: Vec<_> = msg_data
        .messages
        .iter()
        .filter(|m| m.content_type == "error")
        .collect();
    assert!(
        errors.is_empty(),
        "interpret-path smtp send produced error message(s): {:?}",
        errors
    );
    let said_true = msg_data
        .messages
        .iter()
        .any(|m| m.content_type == "text" && m.content["text"].as_str() == Some("true"));
    assert!(
        said_true,
        "expected smtp send to return true through interpret, messages: {:?}",
        msg_data.messages
    );
    mock.join().unwrap();
}

#[tokio::test]
async fn smtp_async_send_rcpt_rejected() {
    let result = run_send(true).await;
    let err = match result {
        Err(e) => e,
        Ok(()) => panic!("expected smtp send to fail when mock rejects RCPT TO"),
    };
    // Assert the failure is specifically the mock's RCPT rejection (550), not an
    // unrelated connection/protocol error that would false-green this test.
    assert!(
        err.contains("550") || err.to_lowercase().contains("no such recipient"),
        "expected RCPT 550 rejection, got a different error: {:?}",
        err
    );
}
