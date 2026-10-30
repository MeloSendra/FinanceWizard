use crate::error::{Result, invalid};
use rusqlite::Connection;
use std::path::Path;

/// Marks a SQLite file as ours, so restore can reject unrelated databases.
pub const APPLICATION_ID: i32 = 0x4657_5A44;

const MIGRATIONS: &[&str] = &[include_str!("../migrations/001_initial.sql")];

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

pub fn schema_version() -> i32 {
    MIGRATIONS.len() as i32
}

pub fn migrate(conn: &Connection) -> Result<()> {
    let current: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current > schema_version() {
        return invalid("This database was created by a newer version of FinanceWizard.");
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let batch = format!(
            "BEGIN;\n{sql}\nPRAGMA user_version = {};\nPRAGMA application_id = {APPLICATION_ID};\nCOMMIT;",
            index + 1
        );
        if let Err(error) = conn.execute_batch(&batch) {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(error.into());
        }
    }
    Ok(())
}

#[cfg(test)]
pub fn test_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    migrate(&conn).unwrap();
    conn
}
