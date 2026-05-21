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

fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: OnceLock<Migrations<'static>> = OnceLock::new();
    MIGRATIONS.get_or_init(|| Migrations::new(vec![M::up(SCHEMA_V1)]))
}

pub fn migrate_conn(conn: &mut Connection) -> Result<()> {
    bridge_legacy_schema(conn)?;
    migrations()
        .to_latest(conn)
        .map_err(|e| BitpartErrorKind::Pool(format!("migrate: {e}")).into())
}

pub async fn migrate(pool: &Pool) -> Result<()> {
    let conn = pool
        .get()
        .await
        .map_err(|e| BitpartErrorKind::Pool(format!("pool get for migrate: {e}")))?;
    conn.interact(|c| migrate_conn(c))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_parses() {
        migrations().validate().expect("schema.sql is valid");
    }

    #[test]
    fn fresh_db_initialises_to_v1() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_conn(&mut conn).unwrap();

        let v: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);

        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 7);
    }

    #[test]
    fn migrator_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_conn(&mut conn).unwrap();
        migrate_conn(&mut conn).unwrap();
        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 7);
    }

    #[test]
    fn bridges_legacy_seaorm_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE bot (id TEXT PRIMARY KEY);
             CREATE TABLE seaql_migrations (version TEXT PRIMARY KEY, applied_at INTEGER);
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
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE name = 'bot'",
                [],
                |_| Ok(true),
            )
            .unwrap();
        assert!(bot_exists);

        migrate_conn(&mut conn).unwrap();
    }
}
