use crate::db::{self, APPLICATION_ID};
use crate::error::{Result, invalid};
use crate::transactions::{self, TransactionFilter};
use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const LAST_BACKUP: &str = "last_backup_at";
const CURRENCY: &str = "currency";
const DEFAULT_CURRENCY: &str = "EUR";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub currency: String,
    pub last_backup_at: Option<String>,
}

fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get(0))
        .optional()?)
}

fn put_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn settings(conn: &Connection) -> Result<Settings> {
    Ok(Settings {
        currency: get_setting(conn, CURRENCY)?.unwrap_or_else(|| DEFAULT_CURRENCY.to_string()),
        last_backup_at: get_setting(conn, LAST_BACKUP)?,
    })
}

pub fn set_currency(conn: &Connection, currency: &str) -> Result<()> {
    if currency.len() != 3 || !currency.chars().all(|c| c.is_ascii_uppercase()) {
        return invalid("The currency must be a three-letter code.");
    }
    put_setting(conn, CURRENCY, currency)
}

/// Copies the database using SQLite's backup API, which stays consistent
/// where copying the live file would not.
fn copy_to_file(conn: &Connection, path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    let mut target = Connection::open(path)?;
    Backup::new(conn, &mut target)?.run_to_completion(256, std::time::Duration::ZERO, None)?;
    Ok(())
}

pub fn create_backup(conn: &Connection, path: &Path, now: &str) -> Result<()> {
    copy_to_file(conn, path)?;
    put_setting(conn, LAST_BACKUP, now)
}

fn check_backup(source: &Connection) -> Result<()> {
    let not_a_backup = "This file is not a FinanceWizard backup.";
    let application_id: i32 = match source.query_row("PRAGMA application_id", [], |row| row.get(0)) {
        Ok(value) => value,
        Err(_) => return invalid(not_a_backup),
    };
    if application_id != APPLICATION_ID {
        return invalid(not_a_backup);
    }
    let version: i32 = source.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > db::schema_version() {
        return invalid("This backup was created by a newer version of FinanceWizard.");
    }
    let integrity: String = source.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return invalid("This backup file is damaged.");
    }
    Ok(())
}

/// Replaces all data with the backup, after saving the current data to
/// `safety_path`. An invalid backup leaves the current data untouched.
pub fn restore_backup(conn: &mut Connection, path: &Path, safety_path: &Path) -> Result<()> {
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    check_backup(&source)?;
    copy_to_file(conn, safety_path)?;
    let last_backup = get_setting(conn, LAST_BACKUP)?;
    Backup::new(&source, conn)?.run_to_completion(256, std::time::Duration::ZERO, None)?;
    db::configure(conn)?;
    db::migrate(conn)?;
    if let Some(value) = last_backup {
        put_setting(conn, LAST_BACKUP, &value)?;
    }
    Ok(())
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn decimal(cents: i64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}

pub fn transactions_csv(conn: &Connection) -> Result<String> {
    // The byte order mark makes Excel read the file as UTF-8.
    let mut csv = String::from("\u{feff}date,type,amount,category,description,recurring_item\r\n");
    let filter = TransactionFilter::default();
    for t in transactions::list(conn, &filter)?.iter().rev() {
        let kind = match t.kind {
            crate::categories::Kind::Income => "income",
            crate::categories::Kind::Expense => "expense",
        };
        let line = [
            t.date.clone(),
            kind.to_string(),
            decimal(t.amount_cents),
            csv_field(&t.category_name),
            csv_field(&t.description),
            csv_field(t.recurring_item_name.as_deref().unwrap_or("")),
        ];
        csv.push_str(&line.join(","));
        csv.push_str("\r\n");
    }
    Ok(csv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::{self, CategoryInput, Kind};
    use crate::db::test_connection;
    use crate::transactions::TransactionInput;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("financewizard-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn seed(conn: &Connection, name: &str, description: &str) {
        let input = CategoryInput { name: name.into(), kind: Kind::Expense, parent_id: None, default_budget_cents: None };
        let category = categories::create(conn, &input).unwrap();
        let input = TransactionInput {
            date: "2026-01-05".into(),
            amount_cents: 12_34,
            description: description.into(),
            category_id: category.id,
            recurring_item_id: None,
        };
        transactions::create(conn, &input).unwrap();
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn restore_brings_back_the_backup_and_keeps_a_safety_copy() {
        let dir = TempDir::new("restore");
        let (backup, safety) = (dir.0.join("backup.sqlite"), dir.0.join("safety.sqlite"));
        let mut conn = test_connection();
        seed(&conn, "Food", "");
        create_backup(&conn, &backup, "2026-01-06T10:00:00").unwrap();
        seed(&conn, "Fun", "");
        assert_eq!(count(&conn), 2);

        restore_backup(&mut conn, &backup, &safety).unwrap();
        assert_eq!(count(&conn), 1);
        assert_eq!(settings(&conn).unwrap().last_backup_at.as_deref(), Some("2026-01-06T10:00:00"));

        let saved = Connection::open(&safety).unwrap();
        assert_eq!(count(&saved), 2);
        let enabled: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0)).unwrap();
        assert_eq!(enabled, 1);
    }

    #[test]
    fn rejects_files_that_are_not_backups_without_touching_data() {
        let dir = TempDir::new("reject");
        let text = dir.0.join("notes.txt");
        fs::write(&text, "not a database, just some text that is long enough").unwrap();
        let other = dir.0.join("other.sqlite");
        Connection::open(&other).unwrap().execute_batch("CREATE TABLE x (y);").unwrap();
        let safety = dir.0.join("safety.sqlite");

        let mut conn = test_connection();
        seed(&conn, "Food", "");
        assert!(restore_backup(&mut conn, &text, &safety).is_err());
        assert!(restore_backup(&mut conn, &other, &safety).is_err());
        assert_eq!(count(&conn), 1);
        assert!(!safety.exists());
    }

    #[test]
    fn rejects_backups_from_a_newer_version() {
        let dir = TempDir::new("newer");
        let backup = dir.0.join("backup.sqlite");
        let conn = test_connection();
        create_backup(&conn, &backup, "now").unwrap();
        Connection::open(&backup).unwrap().pragma_update(None, "user_version", 999).unwrap();
        let mut conn = test_connection();
        assert!(restore_backup(&mut conn, &backup, &dir.0.join("safety.sqlite")).is_err());
    }

    #[test]
    fn csv_escapes_special_characters_and_formats_amounts() {
        let conn = test_connection();
        seed(&conn, "Food", "Lunch, \"big\" one");
        let csv = transactions_csv(&conn).unwrap();
        assert!(csv.contains("2026-01-05,expense,12.34,Food,\"Lunch, \"\"big\"\" one\",\r\n"));
    }

    #[test]
    fn decimal_keeps_two_digits() {
        assert_eq!(decimal(5), "0.05");
        assert_eq!(decimal(120_00), "120.00");
    }
}
