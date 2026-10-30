use crate::db::{self, APPLICATION_ID};
use crate::error::{AppError, Result, invalid};
use crate::transactions::{self, TransactionFilter};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const LAST_BACKUP: &str = "last_backup_at";
const CURRENCY: &str = "currency";
const DEFAULT_CURRENCY: &str = "EUR";
const MIN_PASSPHRASE_LENGTH: usize = 12;

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

fn check_passphrase(passphrase: &str) -> Result<()> {
    if passphrase.chars().count() < MIN_PASSPHRASE_LENGTH {
        return invalid(format!("The passphrase must be at least {MIN_PASSPHRASE_LENGTH} characters."));
    }
    Ok(())
}

/// Backups carry their own salt and need only the passphrase, not `vault.json`,
/// so they can be restored on another computer.
pub fn create_backup(conn: &Connection, path: &Path, passphrase: &str, now: &str) -> Result<()> {
    check_passphrase(passphrase)?;
    let temporary = db::sidecar(path, ".new");
    db::export_to(conn, &temporary, passphrase)?;
    fs::rename(&temporary, path)?;
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

/// Plaintext files are 0.1.0 backups. Safety copies use the database key, and
/// anything else needs the passphrase it was saved with. Opened with the default
/// flags because the attached export target inherits them, and must be creatable.
fn open_source(path: &Path, key: &str, passphrase: Option<&str>) -> Result<Connection> {
    let source = Connection::open(path)?;
    if !db::is_plaintext(path)? {
        db::apply_key(&source, passphrase.unwrap_or(key))?;
    }
    Ok(source)
}

pub fn needs_passphrase(path: &Path, key: &str) -> Result<bool> {
    match open_source(path, key, None) {
        Ok(_) => Ok(false),
        Err(AppError::WrongKey) => Ok(true),
        Err(error) => Err(error),
    }
}

/// Replaces all data with the backup, after saving the current data to
/// `safety_path`. An invalid backup leaves the current data untouched.
pub fn restore_backup(
    conn: &mut Connection,
    database: &Path,
    key: &str,
    path: &Path,
    passphrase: Option<&str>,
    safety_path: &Path,
) -> Result<()> {
    let source = open_source(path, key, passphrase)?;
    check_backup(&source)?;
    db::export_to(conn, safety_path, key)?;
    let last_backup = get_setting(conn, LAST_BACKUP)?;
    let temporary = db::sidecar(database, ".restoring");
    db::export_to(&source, &temporary, key)?;
    drop(source);

    // The database file can only be replaced while nothing has it open.
    drop(std::mem::replace(conn, Connection::open_in_memory()?));
    let replaced = fs::rename(&temporary, database);
    *conn = db::open(database, key)?;
    replaced?;
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
    use crate::db::tests::TempDir;
    use crate::transactions::TransactionInput;

    const KEY: &str = "x'00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff'";
    const PASSPHRASE: &str = "correct horse battery";

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

    fn restore(conn: &mut Connection, dir: &TempDir, backup: &Path, passphrase: Option<&str>) -> Result<()> {
        restore_backup(conn, &dir.0.join("live.sqlite"), KEY, backup, passphrase, &dir.0.join("safety.sqlite"))
    }

    fn live(dir: &TempDir) -> Connection {
        db::open(&dir.0.join("live.sqlite"), KEY).unwrap()
    }

    #[test]
    fn restore_brings_back_the_backup_and_keeps_a_safety_copy() {
        let dir = TempDir::new("restore");
        let backup = dir.0.join("backup.sqlite");
        let mut conn = live(&dir);
        seed(&conn, "Food", "");
        create_backup(&conn, &backup, PASSPHRASE, "2026-01-06T10:00:00").unwrap();
        seed(&conn, "Fun", "");
        assert_eq!(count(&conn), 2);

        restore(&mut conn, &dir, &backup, Some(PASSPHRASE)).unwrap();
        assert_eq!(count(&conn), 1);
        assert_eq!(settings(&conn).unwrap().last_backup_at.as_deref(), Some("2026-01-06T10:00:00"));

        let saved = Connection::open(dir.0.join("safety.sqlite")).unwrap();
        db::apply_key(&saved, KEY).unwrap();
        assert_eq!(count(&saved), 2);
        let enabled: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0)).unwrap();
        assert_eq!(enabled, 1);

        // The replaced file is the one the app reopens.
        seed(&conn, "After", "");
        drop(conn);
        assert_eq!(count(&live(&dir)), 2);
    }

    #[test]
    fn backups_are_encrypted_and_need_their_passphrase() {
        let dir = TempDir::new("passphrase");
        let backup = dir.0.join("backup.sqlite");
        let mut conn = live(&dir);
        seed(&conn, "Food", "a secret purchase");
        create_backup(&conn, &backup, PASSPHRASE, "now").unwrap();

        assert!(!db::is_plaintext(&backup).unwrap());
        assert!(needs_passphrase(&backup, KEY).unwrap());
        assert!(matches!(restore(&mut conn, &dir, &backup, Some("the wrong passphrase")), Err(AppError::WrongKey)));
        assert!(matches!(restore(&mut conn, &dir, &backup, None), Err(AppError::WrongKey)));
        assert!(!dir.0.join("safety.sqlite").exists());
    }

    #[test]
    fn short_passphrases_are_refused() {
        let dir = TempDir::new("short");
        let backup = dir.0.join("backup.sqlite");
        assert!(create_backup(&live(&dir), &backup, "too short", "now").is_err());
        assert!(!backup.exists());
    }

    #[test]
    fn a_safety_copy_restores_with_the_database_key() {
        let dir = TempDir::new("safety");
        let copy = dir.0.join("copy.sqlite");
        let mut conn = live(&dir);
        seed(&conn, "Food", "");
        db::export_to(&conn, &copy, KEY).unwrap();
        seed(&conn, "Fun", "");

        assert!(!needs_passphrase(&copy, KEY).unwrap());
        restore(&mut conn, &dir, &copy, None).unwrap();
        assert_eq!(count(&conn), 1);
    }

    #[test]
    fn a_plaintext_backup_from_the_first_version_restores_encrypted() {
        let dir = TempDir::new("plain-backup");
        let backup = dir.0.join("old.sqlite");
        let old = Connection::open(&backup).unwrap();
        db::configure(&old).unwrap();
        db::migrate(&old).unwrap();
        seed(&old, "Food", "");
        drop(old);

        let mut conn = live(&dir);
        assert!(!needs_passphrase(&backup, KEY).unwrap());
        restore(&mut conn, &dir, &backup, None).unwrap();
        assert_eq!(count(&conn), 1);
        drop(conn);
        assert!(!db::is_plaintext(&dir.0.join("live.sqlite")).unwrap());
    }

    #[test]
    fn rejects_files_that_are_not_backups_without_touching_data() {
        let dir = TempDir::new("reject");
        let text = dir.0.join("notes.txt");
        fs::write(&text, "not a database, just some text that is long enough").unwrap();
        let other = dir.0.join("other.sqlite");
        Connection::open(&other).unwrap().execute_batch("CREATE TABLE x (y);").unwrap();

        let mut conn = live(&dir);
        seed(&conn, "Food", "");
        assert!(restore(&mut conn, &dir, &text, Some(PASSPHRASE)).is_err());
        assert!(restore(&mut conn, &dir, &other, None).is_err());
        assert_eq!(count(&conn), 1);
        assert!(!dir.0.join("safety.sqlite").exists());
    }

    #[test]
    fn rejects_backups_from_a_newer_version() {
        let dir = TempDir::new("newer");
        let backup = dir.0.join("backup.sqlite");
        let mut conn = live(&dir);
        create_backup(&conn, &backup, PASSPHRASE, "now").unwrap();
        let copy = Connection::open(&backup).unwrap();
        db::apply_key(&copy, PASSPHRASE).unwrap();
        copy.pragma_update(None, "user_version", 999).unwrap();
        drop(copy);
        assert!(restore(&mut conn, &dir, &backup, Some(PASSPHRASE)).is_err());
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
