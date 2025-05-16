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
