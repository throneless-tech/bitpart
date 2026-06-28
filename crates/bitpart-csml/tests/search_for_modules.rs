// Integration test for the async `search_for_modules`.
// `search_for_modules` is `async fn`; it fetches each `Module` whose `flow` is None from
// `module.url` over reqwest and stores the downloaded CSML as `module.flow`. This test
// drives it against a local tiny_http mock serving valid CSML flow text and asserts the
// module resolves. Hermetic: ephemeral-port mock, `.no_proxy()` client (see lib.rs), no
// live network / DNS.

use std::thread;

use bitpart_csml::data::csml_bot::{CsmlBot, Module};
use bitpart_csml::search_for_modules;

use tiny_http::{Response, Server};

// Spawns an http mock on an ephemeral port that answers exactly one request with the
// given body, then exits. Mirrors `spawn_http_server` in http_builtin_server.rs.
fn spawn_module_server(body: &'static str) -> (String, thread::JoinHandle<()>) {
    let server = Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let url = format!("http://{}", addr);

    let handle = thread::spawn(move || {
        if let Ok(request) = server.recv() {
            let _ = request.respond(Response::from_string(body));
        }
    });

    (url, handle)
}

#[test]
fn search_for_modules_resolves_module_flow() {
    let flow_content = "start:\n    say \"from module\"\n    goto end\n";
    let (url, handle) = spawn_module_server(flow_content);

    let module = Module {
        name: "my_module".to_owned(),
        url: Some(url),
        auth: None,
        version: "latest".to_owned(),
        flow: None,
    };

    let mut bot = CsmlBot::new(
        "id",
        "bot",
        None,
        vec![],
        None,
        None,
        "flow",
        None,
        None,
        None,
        Some(vec![module]),
        None,
    );

    // search_for_modules is async; block on it with a local runtime (same pattern the
    // support/tools.rs interpret helpers use).
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(search_for_modules(&mut bot));
    assert!(result.is_ok(), "search_for_modules failed: {:?}", result);

    let modules = bot.modules.expect("modules present");
    let module = &modules[0];
    let flow = module
        .flow
        .as_ref()
        .expect("module.flow must be Some after download");
    assert_eq!(
        flow.content, flow_content,
        "downloaded CSML must match the mock body"
    );
    assert_eq!(flow.name, "my_module");

    handle.join().unwrap();
}

// A module that already has a flow must be left untouched (no fetch attempted) — proven by
// pointing at a dead url that would error if a request were made.
#[test]
fn search_for_modules_skips_already_downloaded() {
    use bitpart_csml::data::csml_flow::CsmlFlow;

    let existing = CsmlFlow::new("my_module", "my_module", "start:\n    goto end\n", vec![]);
    let module = Module {
        name: "my_module".to_owned(),
        // unroutable url: if search_for_modules tried to fetch, it would Err.
        url: Some("http://127.0.0.1:1".to_owned()),
        auth: None,
        version: "latest".to_owned(),
        flow: Some(existing),
    };

    let mut bot = CsmlBot::new(
        "id",
        "bot",
        None,
        vec![],
        None,
        None,
        "flow",
        None,
        None,
        None,
        Some(vec![module]),
        None,
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(search_for_modules(&mut bot));
    assert!(
        result.is_ok(),
        "already-downloaded module must be skipped, not re-fetched: {:?}",
        result
    );
}
