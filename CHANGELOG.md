## 2.0.2 (2026-11-13)

### Fix

- Set trace parent in websocket message for logging purposes.

## 2.0.1 (2026-11-13)

### Fix

- add user-count metrics to otel.
- re-add quickcheck_macros as dev dependency for presage store.
- Add additional spans.
- Update for Rust 1.100.0 and add total bots metric.
- Update connection_status OTEL metric to be easier to chart in Grafana.

## 2.0.0 (2026-11-06)

### Feat

- Allow for replying to CAPTCHA challenges.
- Better rate limiters.
- Better group support.
- Allow setting a disappearing message timer on bot creation.
- Upgrade presage dependencies to newest version.
- Add attachment handling.
- Expose per-bot metrics via OTel and by describe command.
- Add delivery receipts.

### Fix

- Attempt to send a reset session for a decrypt error from an unknown session.
- Tighten up logging verbosity so potentially sensitive data is only in debug or above.
- Send correct failure message to dashboard, omit UUIDs from otel spans.
- Override existing disappearing message timer if bot default is shorter.
- Delete additional records on bot deletion, add additional debug logs around disappearing message timers.
- Handle empty messages with attachments.
- Make sure opentelemetry can be enabled from config file or environment variable, and add regression test.
- Update Cargo.lock so async-trait is v1.92 to work around clippy error about double must_use with Rust v1.99
- Show channel status in cli.
- Update bitpart-csml formatting for Rust 2024
- Update Github Actions.
- Simplify CLI interface and properly handle multiple Signal channels per bot.
- Add more thorough deletions and properly handle disappearing messages.
- **ci**: Make sure release notes are appended to release, trigger builds from correct branch.

## 1.2.0 (2026-07-07)

### Feat

- Bring CSML fork in-tree.

### Fix

- Fix flakey bitpart-csml tests.
- Implement stub for thread_for_sender_and_timestamp
- Update dependencies and add support for AccountEntropyPool.

## 1.1.0 (2026-06-09)

### Feat

- Migrate old tables to new format, and update presage dependencies.
- Replace seaorm with rusqlite.
- Code cleanup, split out API to be clearer and re-enable CLI build.
- SignalManager now respects backpressure. Some additional cleanup to tests and socket dispatch.

### Fix

- Point presage dependency at our fork.
- Redact additonal fields from debugging and move some tracing logs to instrumentation.
- Move some of the extraneous threads to tasks instead.

## 1.0.7 (2026-03-01)

### Fix

- Increase retry timeout for restarting signal manager.

## 1.0.6 (2026-03-01)

### Fix

- Update README
- Add additional usage documentation to the README

## 1.0.5 (2026-02-27)

### Fix

- Possible fix for long-running Signal timeouts.

## 1.0.4 (2025-09-05)

### Fix

- Add additional usage documentation to the README

## 1.0.3 (2025-09-04)

### Fix

- Make delete commands return errors if channel/bot is not found.
- Shutdown receive task when channel or its associated bot is deleted.
- Send initial sync message after linking.
- Be more resilient to disconnections or message errors.
- Quick fix for attachment filename sanitization.

## 1.0.2 (2025-08-27)

### Fix

- Compare authorization header in constant time. Shout out to @redshiftzero for catching this.

## 1.0.1 (2025-08-25)

### Fix

- Properly filter whisper messages by bot_id.

## 1.0.0 (2025-08-14)

### Fix

- Fmt fixes.
- Blood for the Clippy god.
- Box up errors.
- Pin Github actions to hashes. Fixes #11
- Update presage to 0.7.0, fixes #23 (hopefully)
- Save new memory value for existing memory. Fixes #22
- Double thread stack size. Fixes #21

## 1.0.0-alpha.12 (2025-05-16)

### Fix

- Don't panic immediately when failing to send message to Signal

## 1.0.0-alpha.11 (2025-05-16)

### Fix

- Delete memory when bot is deleted

## 1.0.0-alpha.10 (2025-05-16)

### Fix

- Don't shout to yourself

## 1.0.0-alpha.9 (2025-05-16)

### Fix

- Don't default events to 'secure'

## 1.0.0-alpha.8 (2025-05-13)

### Fix

- properly serialize memories as their appropriate types instead of as strings
- Remove extraneous unwrap()'s from bitpart and presage-store

## 1.0.0-alpha.7 (2025-05-09)

### Fix

- allow building releases on all pushes to main branch
- Keep duplicate memories from being saved to the database.

## 1.0.0-alpha.6 (2025-05-06)

## 1.0.0-alpha.5 (2025-05-06)

### Fix

- Strip extra quotes from memory strings.
- Fix inclusion of built-in functions in validation.
- Improve CLI response handling.
- Update Cargo.lock for newer package version

## 1.0.0-alpha.4 (2025-05-05)

### Fix

- Change commitizen workflow to treat as alpha
- Change commitizen version provider to Cargo.

## 1.0.0-alpha.3 (2025-05-04)

### Fix

- Add Cargo.lock to repository.

## 1.0.0-alpha.2 (2025-05-03)

### Fix

- Flip conditional on github release workflow.
- **signal**: Automatically start newly-linked channels.
- **signal**: Fix how session identifiers are deserialized when determining device list.
- **signal**: Remove extra contacts sync job, it was causing connection problems.

## 1.0.0-alpha.1 (2025-04-29)

## 1.0.0 (2025-04-28)
