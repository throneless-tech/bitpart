// Bitpart
// Copyright (C) 2025 Throneless Tech

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::sync::OnceLock;

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

use crate::db::Pool;
use crate::error::{BitpartErrorKind, Result};

const SCHEMA_V1: &str = include_str!("schema.sql");
const SCHEMA_V2: &str = include_str!("schema_v2.sql");

fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: OnceLock<Migrations<'static>> = OnceLock::new();
    MIGRATIONS.get_or_init(|| Migrations::new(vec![M::up(SCHEMA_V1), M::up(SCHEMA_V2)]))
}

pub fn migrate_conn(conn: &mut Connection) -> Result<()> {
    bridge_legacy_schema(conn)?;

    let current_version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(BitpartErrorKind::Rusqlite)?;

    let legacy_rows: Option<Vec<(String, String, String, String)>> = if current_version == 1 {
        Some(read_v1_channel_state(conn)?)
    } else {
        None
    };

    migrations()
        .to_latest(conn)
        .map_err(|e| BitpartErrorKind::Pool(format!("migrate: {e}")))?;

    if let Some(rows) = legacy_rows {
        write_v1_rows_to_v2(conn, rows)?;
    }

    Ok(())
}

pub async fn migrate(pool: &Pool) -> Result<()> {
    let conn = pool
        .get()
        .await
        .map_err(|e| BitpartErrorKind::Pool(format!("pool get for migrate: {e}")))?;
    conn.interact(migrate_conn)
        .await
        .map_err(|e| BitpartErrorKind::Pool(format!("interact for migrate: {e}")))??;
    Ok(())
}

fn bridge_legacy_schema(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(BitpartErrorKind::Rusqlite)?;
    if current != 0 {
        return Ok(());
    }

    let has_legacy_marker: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'seaql_migrations'",
            [],
            |_| Ok(true),
        )
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(false),
            other => Err(other),
        })
        .map_err(BitpartErrorKind::Rusqlite)?;

    if !has_legacy_marker {
        return Ok(());
    }

    let tx = conn.transaction().map_err(BitpartErrorKind::Rusqlite)?;
    tx.pragma_update(None, "user_version", 1)
        .map_err(BitpartErrorKind::Rusqlite)?;
    tx.execute("DROP TABLE seaql_migrations", [])
        .map_err(BitpartErrorKind::Rusqlite)?;
    tx.commit().map_err(BitpartErrorKind::Rusqlite)?;

    Ok(())
}

fn read_v1_channel_state(conn: &mut Connection) -> Result<Vec<(String, String, String, String)>> {
    let mut stmt = conn
        .prepare("SELECT channel_id, tree, key, value FROM channel_state")
        .map_err(BitpartErrorKind::Rusqlite)?;

    let rows_result: std::result::Result<Vec<(String, String, String, String)>, rusqlite::Error> =
        stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect();

    Ok(rows_result.map_err(BitpartErrorKind::Rusqlite)?)
}

fn write_v1_rows_to_v2(
    conn: &mut Connection,
    rows: Vec<(String, String, String, String)>,
) -> Result<()> {
    let tx = conn.transaction().map_err(BitpartErrorKind::Rusqlite)?;

    for (channel_id, tree, key, value) in rows {
        match tree.as_str() {
            // ACI Protocol Trees - rotation-aware routing per ADR
            "identities" => {
                insert_identity(&tx, &channel_id, false, &key, &value)?;
            }
            "sessions" => {
                insert_session(&tx, "signal_sessions", &channel_id, &key, &value)?;
            }
            "pre_keys" => {
                insert_pre_key(&tx, "signal_pre_keys", &channel_id, &key, &value)?;
            }
            "sender_keys" => {
                // ROTATED: sender_keys -> signal_signed_pre_keys
                insert_pre_key(&tx, "signal_signed_pre_keys", &channel_id, &key, &value)?;
            }
            "signed_pre_keys" => {
                // ROTATED: signed_pre_keys -> signal_kyber_pre_keys (is_last_resort=0)
                insert_kyber_pre_key(
                    &tx,
                    "signal_kyber_pre_keys",
                    &channel_id,
                    &key,
                    &value,
                    false,
                )?;
            }
            "kyber_pre_keys_last_resort" => {
                insert_kyber_pre_key(
                    &tx,
                    "signal_kyber_pre_keys",
                    &channel_id,
                    &key,
                    &value,
                    true,
                )?;
            }
            "kyber_pre_keys" => {
                // ROTATED: kyber_pre_keys -> signal_sender_keys
                insert_sender_key(&tx, "signal_sender_keys", &channel_id, &key, &value)?;
            }
            "base_keys_seen" => {
                insert_base_keys_seen(&tx, &channel_id, false, &key, &value)?;
            }
            "state" => {
                insert_state(&tx, "signal_state", &channel_id, &key, &value)?;
            }

            "pni_pre_keys" => {
                insert_pre_key(&tx, "signal_pni_pre_keys", &channel_id, &key, &value)?;
            }
            "pni_sender_keys" => {
                // ROTATED: pni_sender_keys -> signal_pni_signed_pre_keys
                insert_pre_key(&tx, "signal_pni_signed_pre_keys", &channel_id, &key, &value)?;
            }
            "pni_signed_pre_keys" => {
                // ROTATED: pni_signed_pre_keys -> signal_pni_kyber_pre_keys (is_last_resort=0)
                insert_kyber_pre_key(
                    &tx,
                    "signal_pni_kyber_pre_keys",
                    &channel_id,
                    &key,
                    &value,
                    false,
                )?;
            }
            "pni_kyber_pre_keys_last_resort" => {
                insert_kyber_pre_key(
                    &tx,
                    "signal_pni_kyber_pre_keys",
                    &channel_id,
                    &key,
                    &value,
                    true,
                )?;
            }
            "pni_kyber_pre_keys" => {
                // ROTATED: pni_kyber_pre_keys -> signal_pni_sender_keys
                insert_sender_key(&tx, "signal_pni_sender_keys", &channel_id, &key, &value)?;
            }
            "pni_sessions" => {
                insert_session(&tx, "signal_pni_sessions", &channel_id, &key, &value)?;
            }
            "pni_state" => {
                insert_state(&tx, "signal_pni_state", &channel_id, &key, &value)?;
            }

            "profiles" => {
                insert_content(
                    &tx,
                    "signal_profiles",
                    &channel_id,
                    &key,
                    &value,
                    "profile_hash",
                )?;
            }
            "profile_keys" => {
                insert_content(
                    &tx,
                    "signal_profile_keys",
                    &channel_id,
                    &key,
                    &value,
                    "uuid",
                )?;
            }
            "profile_avatars" => {
                insert_content(
                    &tx,
                    "signal_profile_avatars",
                    &channel_id,
                    &key,
                    &value,
                    "profile_hash",
                )?;
            }
            "contacts" => {
                insert_content(&tx, "signal_contacts", &channel_id, &key, &value, "uuid")?;
            }
            "groups" => {
                insert_content(
                    &tx,
                    "signal_groups",
                    &channel_id,
                    &key,
                    &value,
                    "master_key",
                )?;
            }
            "group_avatars" => {
                insert_content(
                    &tx,
                    "signal_group_avatars",
                    &channel_id,
                    &key,
                    &value,
                    "master_key",
                )?;
            }
            "sticker_packs" => {
                insert_content(
                    &tx,
                    "signal_sticker_packs",
                    &channel_id,
                    &key,
                    &value,
                    "pack_id",
                )?;
            }

            // Dynamic threads:* trees -> signal_messages
            tree_name if tree_name.starts_with("threads:") => {
                let thread_id = &tree_name[8..]; // Strip "threads:" prefix
                insert_message(&tx, &channel_id, thread_id, &key, &value)?;
            }

            _ => {
                return Err(
                    BitpartErrorKind::Pool(format!("unexpected legacy tree: {}", tree)).into(),
                );
            }
        }
    }

    tx.commit().map_err(BitpartErrorKind::Rusqlite)?;
    Ok(())
}

// Legacy keys were stored via `serde_json::to_string(key.as_ref())`, which
// produces a JSON byte array (e.g. `[97,100,...]`). Decode back to the
// original bytes and then to UTF-8 for string-shaped keys.
fn decode_legacy_string_key(key_json: &str, field: &str) -> Result<String> {
    let bytes: Vec<u8> = serde_json::from_str(key_json)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode {field} key bytes: {e}")))?;
    String::from_utf8(bytes)
        .map_err(|e| BitpartErrorKind::Pool(format!("{field} key not UTF-8: {e}")).into())
}

fn insert_identity(
    tx: &rusqlite::Transaction,
    channel_id: &str,
    is_pni: bool,
    address_json: &str,
    value_json: &str,
) -> Result<()> {
    let address = decode_legacy_string_key(address_json, "identity")?;
    let identity_key_bytes: Vec<u8> = serde_json::from_str::<Vec<u8>>(value_json)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode identity key: {e}")))?;

    tx.execute(
        "INSERT OR REPLACE INTO signal_identities (channel_id, is_pni, address, identity_key) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![channel_id, is_pni as i64, &address, identity_key_bytes]
    )
    .map_err(BitpartErrorKind::Rusqlite)?;
    Ok(())
}

fn insert_session(
    tx: &rusqlite::Transaction,
    table: &str,
    channel_id: &str,
    address_json: &str,
    value_json: &str,
) -> Result<()> {
    let address = decode_legacy_string_key(address_json, "session")?;
    let session_data: Vec<u8> = serde_json::from_str::<Vec<u8>>(value_json)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode session: {e}")))?;

    let sql = format!(
        "INSERT OR REPLACE INTO {} (channel_id, address, session_data) VALUES (?1, ?2, ?3)",
        table
    );
    tx.execute(&sql, rusqlite::params![channel_id, &address, session_data])
        .map_err(BitpartErrorKind::Rusqlite)?;
    Ok(())
}

fn insert_pre_key(
    tx: &rusqlite::Transaction,
    table: &str,
    channel_id: &str,
    key_bytes: &str,
    value_json: &str,
) -> Result<()> {
    let key_u32_bytes: Vec<u8> = serde_json::from_str::<Vec<u8>>(key_bytes)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode pre_key id: {e}")))?;

    if key_u32_bytes.len() != 4 {
        return Err(BitpartErrorKind::Pool(format!(
            "invalid key length: expected 4, got {}",
            key_u32_bytes.len()
        ))
        .into());
    }

    let key_id = u32::from_be_bytes([
        key_u32_bytes[0],
        key_u32_bytes[1],
        key_u32_bytes[2],
        key_u32_bytes[3],
    ]) as i64;

    let record_data: Vec<u8> = serde_json::from_str::<Vec<u8>>(value_json)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode pre_key record: {e}")))?;

    let sql = format!(
        "INSERT OR REPLACE INTO {} (channel_id, key_id, record_data) VALUES (?1, ?2, ?3)",
        table
    );
    tx.execute(&sql, rusqlite::params![channel_id, key_id, record_data])
        .map_err(BitpartErrorKind::Rusqlite)?;
    Ok(())
}

fn insert_kyber_pre_key(
    tx: &rusqlite::Transaction,
    table: &str,
    channel_id: &str,
    key_bytes: &str,
    value_json: &str,
    is_last_resort: bool,
) -> Result<()> {
    let key_u32_bytes: Vec<u8> = serde_json::from_str::<Vec<u8>>(key_bytes)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode kyber_pre_key id: {e}")))?;

    if key_u32_bytes.len() != 4 {
        return Err(BitpartErrorKind::Pool(format!(
            "invalid key length: expected 4, got {}",
            key_u32_bytes.len()
        ))
        .into());
    }

    let key_id = u32::from_be_bytes([
        key_u32_bytes[0],
        key_u32_bytes[1],
        key_u32_bytes[2],
        key_u32_bytes[3],
    ]) as i64;

    // Legacy format: {"record": [bytes], "is_last_resort": bool}
    let value: serde_json::Value = serde_json::from_str(value_json)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode kyber_pre_key record: {e}")))?;
    let record_data: Vec<u8> = serde_json::from_value(
        value
            .get("record")
            .ok_or_else(|| BitpartErrorKind::Pool("kyber_pre_key missing record field".into()))?
            .clone(),
    )
    .map_err(|e| BitpartErrorKind::Pool(format!("decode kyber_pre_key record bytes: {e}")))?;

    let sql = format!(
        "INSERT OR REPLACE INTO {} (channel_id, key_id, record_data, is_last_resort) VALUES (?1, ?2, ?3, ?4)",
        table
    );
    tx.execute(
        &sql,
        rusqlite::params![channel_id, key_id, record_data, is_last_resort as i64],
    )
    .map_err(BitpartErrorKind::Rusqlite)?;
    Ok(())
}

fn insert_sender_key(
    tx: &rusqlite::Transaction,
    table: &str,
    channel_id: &str,
    sender_key_json: &str,
    value_json: &str,
) -> Result<()> {
    let sender_key = decode_legacy_string_key(sender_key_json, "sender_key")?;
    let record_data: Vec<u8> = serde_json::from_str::<Vec<u8>>(value_json)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode sender_key record: {e}")))?;

    let sql = format!(
        "INSERT OR REPLACE INTO {} (channel_id, sender_key, record_data) VALUES (?1, ?2, ?3)",
        table
    );
    tx.execute(
        &sql,
        rusqlite::params![channel_id, &sender_key, record_data],
    )
    .map_err(BitpartErrorKind::Rusqlite)?;
    Ok(())
}

fn insert_base_keys_seen(
    tx: &rusqlite::Transaction,
    channel_id: &str,
    is_pni: bool,
    key_bytes: &str,
    value_json: &str,
) -> Result<()> {
    let key_u32_bytes: Vec<u8> = serde_json::from_str::<Vec<u8>>(key_bytes)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode base_keys_seen id: {e}")))?;

    if key_u32_bytes.len() != 4 {
        return Err(BitpartErrorKind::Pool(format!(
            "invalid key length: expected 4, got {}",
            key_u32_bytes.len()
        ))
        .into());
    }

    let kyber_pre_key_id = u32::from_be_bytes([
        key_u32_bytes[0],
        key_u32_bytes[1],
        key_u32_bytes[2],
        key_u32_bytes[3],
    ]) as i64;

    // Value is BaseKeysSeen JSON - extract signed_pre_key_id and base_key bytes
    let base_keys_seen: serde_json::Value =
        serde_json::from_str::<serde_json::Value>(value_json)
            .map_err(|e| BitpartErrorKind::Pool(format!("decode BaseKeysSeen: {e}")))?;

    let signed_pre_key_id = base_keys_seen
        .get("signed_pre_key_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| {
            BitpartErrorKind::Pool("decode signed_pre_key_id: missing or invalid".into())
        })?;

    let base_key: Vec<u8> = base_keys_seen
        .get("base_key")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| BitpartErrorKind::Pool("decode base_key: missing or invalid".into()))?;

    tx.execute(
        "INSERT OR REPLACE INTO signal_base_keys_seen (channel_id, is_pni, kyber_pre_key_id, signed_pre_key_id, base_key) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![channel_id, is_pni as i64, kyber_pre_key_id, signed_pre_key_id, base_key]
    )
    .map_err(BitpartErrorKind::Rusqlite)?;
    Ok(())
}

fn insert_state(
    tx: &rusqlite::Transaction,
    table: &str,
    channel_id: &str,
    key_json: &str,
    value_json: &str,
) -> Result<()> {
    let key_bytes: Vec<u8> = serde_json::from_str(key_json)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode state key bytes: {e}")))?;
    let key = String::from_utf8(key_bytes)
        .map_err(|e| BitpartErrorKind::Pool(format!("state key not UTF-8: {e}")))?;

    let value_bytes: Vec<u8> = match key.as_str() {
        "registration" => value_json.as_bytes().to_vec(),
        "sender_certificate" | "master" => serde_json::from_str::<Vec<u8>>(value_json)
            .map_err(|e| BitpartErrorKind::Pool(format!("decode state bytes: {e}")))?,
        "aci_identity_key_pair" | "pni_identity_key_pair" => {
            let s: String = serde_json::from_str(value_json)
                .map_err(|e| BitpartErrorKind::Pool(format!("decode state string: {e}")))?;
            s.into_bytes()
        }
        _ => value_json.as_bytes().to_vec(),
    };

    let sql = format!(
        "INSERT OR REPLACE INTO {} (channel_id, key, value) VALUES (?1, ?2, ?3)",
        table
    );
    tx.execute(&sql, rusqlite::params![channel_id, &key, value_bytes])
        .map_err(BitpartErrorKind::Rusqlite)?;
    Ok(())
}

fn insert_content(
    tx: &rusqlite::Transaction,
    table: &str,
    channel_id: &str,
    key_json: &str,
    value_json: &str,
    key_column: &str,
) -> Result<()> {
    let key_bytes: Vec<u8> = serde_json::from_str::<Vec<u8>>(key_json)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode content key: {e}")))?;

    let value_bytes: Vec<u8> = if table == "signal_profile_keys" {
        serde_json::from_str::<Vec<u8>>(value_json)
            .map_err(|e| BitpartErrorKind::Pool(format!("decode profile_key value: {e}")))?
    } else {
        value_json.as_bytes().to_vec()
    };

    let data_column = if table.contains("profile") && table.contains("key") {
        "profile_key"
    } else if table.contains("avatar") {
        "avatar_data"
    } else if table.contains("contact") {
        "contact_data"
    } else if table.contains("group") && !table.contains("avatar") {
        "group_data"
    } else if table.contains("sticker") {
        "pack_data"
    } else {
        "profile_data"
    };

    let sql = format!(
        "INSERT OR REPLACE INTO {} (channel_id, {}, {}) VALUES (?1, ?2, ?3)",
        table, key_column, data_column
    );

    if key_column == "profile_hash" {
        let key_str = String::from_utf8_lossy(&key_bytes).to_string();
        tx.execute(&sql, rusqlite::params![channel_id, key_str, value_bytes])
    } else {
        tx.execute(&sql, rusqlite::params![channel_id, key_bytes, value_bytes])
    }
    .map_err(BitpartErrorKind::Rusqlite)?;

    Ok(())
}

fn insert_message(
    tx: &rusqlite::Transaction,
    channel_id: &str,
    thread_id: &str,
    key_bytes: &str,
    value_json: &str,
) -> Result<()> {
    let ts_u64_bytes: Vec<u8> = serde_json::from_str::<Vec<u8>>(key_bytes)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode message timestamp: {e}")))?;

    if ts_u64_bytes.len() != 8 {
        return Err(BitpartErrorKind::Pool(format!(
            "invalid timestamp length: expected 8, got {}",
            ts_u64_bytes.len()
        ))
        .into());
    }

    let timestamp = u64::from_be_bytes([
        ts_u64_bytes[0],
        ts_u64_bytes[1],
        ts_u64_bytes[2],
        ts_u64_bytes[3],
        ts_u64_bytes[4],
        ts_u64_bytes[5],
        ts_u64_bytes[6],
        ts_u64_bytes[7],
    ]) as i64;

    let content_data: Vec<u8> = serde_json::from_str::<Vec<u8>>(value_json)
        .map_err(|e| BitpartErrorKind::Pool(format!("decode message content: {e}")))?;

    tx.execute(
        "INSERT OR REPLACE INTO signal_messages (channel_id, thread_id, timestamp, content_data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![channel_id, thread_id, timestamp, content_data]
    )
    .map_err(BitpartErrorKind::Rusqlite)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_parses() {
        migrations().validate().expect("schema.sql is valid");
    }

    #[test]
    fn fresh_db_initialises_to_v2() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_conn(&mut conn).unwrap();

        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 2);

        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 28);

        let channel_state_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE name = 'channel_state'",
                [],
                |_| Ok(true),
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(other),
            })
            .unwrap();
        assert!(!channel_state_exists);
    }

    #[test]
    fn migrator_is_idempotent_v2() {
        let mut conn = Connection::open_in_memory().unwrap();

        migrate_conn(&mut conn).unwrap();

        let v1: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v1, 2);

        let table_count_1: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        conn.execute(
            "INSERT INTO signal_identities (channel_id, is_pni, address, identity_key) VALUES ('test', 0, 'addr1', ?1)",
            [b"test_key".as_slice()]
        ).unwrap();

        migrate_conn(&mut conn).unwrap();

        let v2: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(
            v2, 2,
            "user_version should stay 2 after idempotent migration"
        );

        let table_count_2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(
            table_count_2, table_count_1,
            "table count should be unchanged after idempotent migration"
        );

        let identity_exists: bool = conn
            .query_row(
                "SELECT 1 FROM signal_identities WHERE channel_id = 'test' AND address = 'addr1'",
                [],
                |_| Ok(true),
            )
            .unwrap();
        assert!(
            identity_exists,
            "existing data should be preserved during idempotent migration"
        );
    }

    #[test]
    fn bridges_legacy_seaorm_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute_batch(
            "CREATE TABLE seaql_migrations (version TEXT PRIMARY KEY, applied_at INTEGER);
             INSERT INTO seaql_migrations (version, applied_at) VALUES ('m20240801_000001', 0);",
        )
        .unwrap();

        bridge_legacy_schema(&mut conn).unwrap();

        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);

        let marker_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE name = 'seaql_migrations'",
                [],
                |_| Ok(true),
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(other),
            })
            .unwrap();
        assert!(!marker_exists);

        let bot_exists: bool = conn
            .query_row("SELECT 1 FROM sqlite_master WHERE name = 'bot'", [], |_| {
                Ok(true)
            })
            .unwrap();
        assert!(bot_exists);

        migrate_conn(&mut conn).unwrap();
    }

    #[test]
    fn bridges_legacy_seaorm_schema_then_v2() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute_batch(
            "CREATE TABLE seaql_migrations (version TEXT PRIMARY KEY, applied_at INTEGER);
             INSERT INTO seaql_migrations (version, applied_at) VALUES ('m20240801_000001', 0);",
        )
        .unwrap();

        migrate_conn(&mut conn).unwrap();

        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 2);

        let marker_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE name = 'seaql_migrations'",
                [],
                |_| Ok(true),
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(other),
            })
            .unwrap();
        assert!(!marker_exists);

        let bot_exists: bool = conn
            .query_row("SELECT 1 FROM sqlite_master WHERE name = 'bot'", [], |_| {
                Ok(true)
            })
            .unwrap();
        assert!(bot_exists);
    }

    #[test]
    fn v1_channel_state_routes_correctly() {
        let mut conn = Connection::open_in_memory().unwrap();

        conn.execute_batch(
            "CREATE TABLE bot (id TEXT PRIMARY KEY);
             CREATE TABLE channel_state (
                 id TEXT PRIMARY KEY,
                 channel_id TEXT NOT NULL,
                 tree TEXT NOT NULL,
                 key TEXT NOT NULL,
                 value TEXT NOT NULL
             );",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();

        let test_data = vec![
            // String-shaped keys (addresses, sender keys) were stored by
            // the legacy code as `serde_json::to_string(key.as_ref())`,
            // which yields a JSON byte-array of the UTF-8 bytes (e.g.
            // "addr1" -> [97,100,100,114,49]).
            (
                "ch1",
                "identities",
                "[97,100,100,114,49]",
                r#"[1,2,3,4,5,6,7,8]"#,
            ),
            ("ch1", "sessions", "[97,100,100,114,49]", r#"[10,11,12,13]"#),
            ("ch1", "pre_keys", r#"[0,0,0,1]"#, r#"[14,15,16,17]"#), // key_id = 1
            ("ch1", "sender_keys", r#"[0,0,0,2]"#, r#"[18,19,20,21]"#), // -> signal_signed_pre_keys, key_id = 2
            // Legacy kyber storage wraps the record in a {record, is_last_resort} object.
            (
                "ch1",
                "signed_pre_keys",
                r#"[0,0,0,3]"#,
                r#"{"record":[22,23,24,25],"is_last_resort":false}"#,
            ), // -> signal_kyber_pre_keys, key_id = 3
            (
                "ch1",
                "kyber_pre_keys_last_resort",
                r#"[0,0,0,4]"#,
                r#"{"record":[26,27,28,29],"is_last_resort":true}"#,
            ), // -> signal_kyber_pre_keys with is_last_resort=1
            (
                "ch1",
                "kyber_pre_keys",
                "[115,101,110,100,101,114,95,97,100,100,114,49]",
                r#"[30,31,32,33]"#,
            ), // -> signal_sender_keys
            (
                "ch1",
                "base_keys_seen",
                r#"[0,0,0,5]"#,
                r#"{"signed_pre_key_id":10,"base_key":[34,35,36,37]}"#,
            ),
            (
                "ch1",
                "state",
                "[108,111,99,97,108,95,97,100,100,114]",
                r#""state_value""#,
            ),
            // PNI Protocol Trees - rotation-aware routing
            ("ch1", "pni_pre_keys", r#"[0,0,0,6]"#, r#"[38,39,40,41]"#),
            ("ch1", "pni_sender_keys", r#"[0,0,0,7]"#, r#"[42,43,44,45]"#), // -> signal_pni_signed_pre_keys
            (
                "ch1",
                "pni_signed_pre_keys",
                r#"[0,0,0,8]"#,
                r#"{"record":[46,47,48,49],"is_last_resort":false}"#,
            ), // -> signal_pni_kyber_pre_keys
            (
                "ch1",
                "pni_kyber_pre_keys_last_resort",
                r#"[0,0,0,9]"#,
                r#"{"record":[50,51,52,53],"is_last_resort":true}"#,
            ), // -> signal_pni_kyber_pre_keys with is_last_resort=1
            (
                "ch1",
                "pni_kyber_pre_keys",
                "[112,110,105,95,115,101,110,100,101,114,95,97,100,100,114,49]",
                r#"[54,55,56,57]"#,
            ), // -> signal_pni_sender_keys
            (
                "ch1",
                "pni_sessions",
                "[112,110,105,95,97,100,100,114,49]",
                r#"[58,59,60,61]"#,
            ),
            (
                "ch1",
                "pni_state",
                "[112,110,105,95,108,111,99,97,108,95,97,100,100,114]",
                r#""pni_state_value""#,
            ),
            (
                "ch1",
                "profiles",
                "[112,114,111,102,105,108,101,95,104,97,115,104,95,49,50,51]",
                r#"[62,63,64,65]"#,
            ),
            (
                "ch1",
                "profile_keys",
                r#"[66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81]"#,
                // ProfileKey: 32 raw bytes (new read path does try_into::<[u8;32]>)
                r#"[82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113]"#,
            ), // 16-byte UUID
            (
                "ch1",
                "profile_avatars",
                "[97,118,97,116,97,114,95,104,97,115,104,95,52,53,54]",
                r#"[86,87,88,89]"#,
            ),
            (
                "ch1",
                "contacts",
                r#"[90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105]"#,
                r#"[106,107,108,109]"#,
            ), // 16-byte UUID
            (
                "ch1",
                "groups",
                r#"[110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141]"#,
                r#"[142,143,144,145]"#,
            ), // 32-byte master key
            (
                "ch1",
                "group_avatars",
                r#"[146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177]"#,
                r#"[178,179,180,181]"#,
            ), // 32-byte master key
            (
                "ch1",
                "sticker_packs",
                r#"[182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197]"#,
                r#"[198,199,200,201]"#,
            ), // 16-byte pack ID
            // Dynamic threads:* trees -> signal_messages
            (
                "ch1",
                "threads:abc123",
                r#"[0,0,0,0,0,0,0,1]"#,
                r#"[202,203,204,205]"#,
            ), // timestamp = 1
            (
                "ch1",
                "threads:def456",
                r#"[0,0,0,0,0,0,0,2]"#,
                r#"[206,207,208,209]"#,
            ), // timestamp = 2
        ];

        for (channel_id, tree, key, value) in test_data {
            conn.execute(
                "INSERT INTO channel_state (id, channel_id, tree, key, value) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![format!("{}-{}-{}", channel_id, tree, key), channel_id, tree, key, value]
            ).unwrap();
        }

        // Run migration
        migrate_conn(&mut conn).unwrap();

        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 2);

        let channel_state_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE name = 'channel_state'",
                [],
                |_| Ok(true),
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(other),
            })
            .unwrap();
        assert!(
            !channel_state_exists,
            "channel_state table should be dropped after migration"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_signed_pre_keys WHERE channel_id = 'ch1' AND key_id = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "ACI sender_keys should route to signal_signed_pre_keys"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_kyber_pre_keys WHERE channel_id = 'ch1' AND key_id = 3 AND is_last_resort = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "ACI signed_pre_keys should route to signal_kyber_pre_keys (normal)"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_sender_keys WHERE channel_id = 'ch1' AND sender_key = 'sender_addr1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "ACI kyber_pre_keys should route to signal_sender_keys"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_pni_signed_pre_keys WHERE channel_id = 'ch1' AND key_id = 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "PNI sender_keys should route to signal_pni_signed_pre_keys"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_pni_kyber_pre_keys WHERE channel_id = 'ch1' AND key_id = 8 AND is_last_resort = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "PNI signed_pre_keys should route to signal_pni_kyber_pre_keys (normal)"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_pni_sender_keys WHERE channel_id = 'ch1' AND sender_key = 'pni_sender_addr1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "PNI kyber_pre_keys should route to signal_pni_sender_keys"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_kyber_pre_keys WHERE channel_id = 'ch1' AND key_id = 4 AND is_last_resort = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "kyber_pre_keys_last_resort should route to signal_kyber_pre_keys with is_last_resort=1"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_pni_kyber_pre_keys WHERE channel_id = 'ch1' AND key_id = 9 AND is_last_resort = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "pni_kyber_pre_keys_last_resort should route to signal_pni_kyber_pre_keys with is_last_resort=1"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_pre_keys WHERE channel_id = 'ch1' AND key_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "pre_keys should route directly to signal_pre_keys"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_sessions WHERE channel_id = 'ch1' AND address = 'addr1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "sessions should route directly to signal_sessions"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_identities WHERE channel_id = 'ch1' AND address = 'addr1' AND is_pni = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "identities should route directly to signal_identities"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_profiles WHERE channel_id = 'ch1' AND profile_hash = 'profile_hash_123'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "profiles should route to signal_profiles");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_profile_keys WHERE channel_id = 'ch1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "profile_keys should route to signal_profile_keys");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_messages WHERE channel_id = 'ch1' AND thread_id = 'abc123' AND timestamp = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "threads:* should route to signal_messages with correct thread_id"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM signal_messages WHERE channel_id = 'ch1' AND thread_id = 'def456' AND timestamp = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "multiple threads should each create separate message rows"
        );
    }
}
