<h1 align="center">
  <br>
  <a href="https://www.csml.dev"><img src="./images/csml-horizontal-whitebg-v3.png?raw=true" alt="CSML" width="200"></a>
  <br>
</h1>

<h4 align="center">The interpreter for CSML, the first programming language dedicated to building chatbots.</h4>

<p align="center">
  <img src="https://github.com/CSML-by-Clevy/csml-engine/workflows/Rust/badge.svg" alt="Rust">
</p>

# bitpart-csml

`bitpart-csml` (crate lib name `bitpart_csml`) is the Rust library that parses, validates, and executes
[CSML (Conversational Standard Meta Language)](https://csml.dev) flows. CSML is a
domain-specific language for chatbots, with an expressive text-only syntax, built-in
memory slots, and rich conversational components (Carousel, Image, Video, Button,
Card, Input, Calendar, ...).

This crate is the language core only. It is **stateless**: it does not connect to a
database, expose an HTTP server, or persist conversation state. Callers drive a single
interpretation turn and consume the resulting messages.

## What it does

- **Parse and lint** CSML flows into an AST.
- **Execute** a step for a given bot, context, and incoming event, producing the
  chatbot's response messages.
- **Stream output** through an optional `std::sync::mpsc::Sender<MSG>` channel, so
  messages can be consumed incrementally as the interpreter runs.

## Public API

| Function | Purpose |
|----------|---------|
| `interpret(bot, context, event, sender) -> MessageData` | Run one interpretation turn. Output is emitted both as the returned `MessageData` and, if `sender` is `Some`, streamed as `MSG` values over the channel. |
| `validate_bot(bot: &CsmlBot) -> CsmlResult` | Lint/validate a bot, returning errors and warnings without executing it. |
| `fold_bot(bot: &CsmlBot) -> String` | Serialize a bot's flows into a single folded representation. |
| `get_steps_from_flow(bot: CsmlBot) -> HashMap<String, Vec<String>>` | List the steps declared in each flow. |
| `search_for_modules(bot: &mut CsmlBot) -> Result<(), String>` | Resolve and inline external CSML modules referenced by the bot. |
| `load_components() -> Result<.., ErrorInfo>` | Load the built-in native components catalogue. |
| `get_step(step_name, flow, ast) -> String` | Compute the source slice of a step (used for step-checksum / hold-resume logic). |

## Example

A minimal CSML flow:

```cpp
start:
  say "Hi, nice to meet you, I'm a demo bot 👋"
  if (name) {
    say "I already know you 😉"
    goto known
  }
  else
    goto name

name:
  say "I'd like to know you better, what's your name?"
  hold
  remember name = event
  goto known

known:
  say "You are {{name}}!"
  goto end
```

Driving it from Rust (see `examples/hello_world.rs` for the full, runnable version).
Note `interpret` is an `async fn` — call it from within a tokio runtime:

```rust
use bitpart_csml::{interpret, validate_bot, load_components};
use bitpart_csml::data::csml_bot::CsmlBot;
use bitpart_csml::data::csml_flow::CsmlFlow;
use bitpart_csml::data::{event::Event, Context};

let flow = CsmlFlow::new("id", "default", &flow_source, Vec::default());
let bot = CsmlBot::new(
    "id", "my_bot", None, vec![flow],
    Some(load_components().unwrap()),
    None, "default", None, None, None, None, None,
);

let result = validate_bot(&bot);
if result.errors.is_some() {
    // handle validation errors
}

// interpret is async; .await it (or block_on) inside a tokio runtime.
let message_data = interpret(bot, context, event, None).await;
```

Runnable examples live in `examples/`. Run them from the repo root:

```
cargo run --example hello_world
```

## Building and testing

```
cargo build
cargo test
```

## Documentation

The CSML language reference is available at https://docs.csml.dev/language.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](./LICENSE).
