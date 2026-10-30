use crate::error::{AppError, Result, invalid};
use chrono::Local;
use rusqlite::{Connection, ErrorCode, params};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Marks a SQLite file as ours, so restore can reject unrelated databases.
pub const APPLICATION_ID: i32 = 0x4657_5A44;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_initial.sql"),
    include_str!("../migrations/002_category_color.sql"),
];
const PLAINTEXT_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// `key` is passed to SQLCipher as is: a passphrase, or `x'<hex>'` for a raw key.
pub fn open(path: &Path, key: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    apply_key(&conn, key)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&conn)?;
    save_before_migrating(&conn, path, key)?;
    migrate(&conn)?;
    Ok(conn)
}

/// Must run before anything else on the connection. SQLCipher only notices a
/// wrong key on the first read, so this reads to find out straight away.
pub fn apply_key(conn: &Connection, key: &str) -> Result<()> {
    conn.pragma_update(None, "key", key)?;
    match conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0)) {
        Ok(_) => Ok(()),
        Err(error) if error.sqlite_error_code() == Some(ErrorCode::NotADatabase) => Err(AppError::WrongKey),
        Err(error) => Err(error.into()),
    }
}

pub fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

pub fn schema_version() -> i32 {
    MIGRATIONS.len() as i32
}

fn user_version(conn: &Connection) -> Result<i32> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    let current = user_version(conn)?;
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

/// Keeps a copy of a database an update is about to change, so a bad
/// migration can be undone by restoring it.
fn save_before_migrating(conn: &Connection, path: &Path, key: &str) -> Result<()> {
    let current = user_version(conn)?;
    if current == 0 || current >= schema_version() {
        return Ok(());
    }
    let directory = path.parent().unwrap_or(Path::new(".")).join("safety-backups");
    fs::create_dir_all(&directory)?;
    let name = format!("before-update-v{current}-{}.sqlite", Local::now().format("%Y-%m-%d-%H%M%S"));
    export_to(conn, &directory.join(name), key)
}

/// True for the unencrypted databases written by version 0.1.0, and for empty
/// files, which SQLite treats as new databases.
pub fn is_plaintext(path: &Path) -> Result<bool> {
    let mut header = Vec::with_capacity(PLAINTEXT_HEADER.len());
    fs::File::open(path)?.take(PLAINTEXT_HEADER.len() as u64).read_to_end(&mut header)?;
    Ok(header.is_empty() || header == PLAINTEXT_HEADER)
}

/// Writes a full copy of the database to a new file encrypted with `key`.
pub fn export_to(conn: &Connection, path: &Path, key: &str) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    let result = export_attached(conn, path, key);
    let detached = conn.execute_batch("DETACH DATABASE export");
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result?;
    Ok(detached?)
}

fn export_attached(conn: &Connection, path: &Path, key: &str) -> Result<()> {
    conn.execute("ATTACH DATABASE ?1 AS export KEY ?2", params![path.to_string_lossy(), key])?;
    conn.query_row("SELECT sqlcipher_export('export')", [], |_| Ok(()))?;
    // sqlcipher_export leaves these for the caller to set.
    let version = user_version(conn)?;
    let application_id: i32 = conn.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    conn.execute_batch(&format!(
        "PRAGMA export.user_version = {version};\nPRAGMA export.application_id = {application_id};"
    ))?;
    Ok(())
}

fn table_counts(conn: &Connection) -> Result<BTreeMap<String, i64>> {
    let mut statement =
        conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?;
    let names = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut counts = BTreeMap::new();
    for name in names {
        let count = conn.query_row(&format!("SELECT COUNT(*) FROM \"{name}\""), [], |row| row.get(0))?;
        counts.insert(name, count);
    }
    Ok(counts)
}

/// `-wal` or `-shm` next to `path`.
pub fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}

/// Copies an unencrypted database to `target` and proves the copy is complete.
/// `source` is left untouched so a failure at any point loses nothing. It goes
/// through SQLite rather than the filesystem since recent data can still be in
/// the `-wal` file.
pub fn encrypt_plaintext(source: &Path, target: &Path, key: &str) -> Result<()> {
    let conn = Connection::open(source)?;
    if user_version(&conn)? > schema_version() {
        return invalid("This database was created by a newer version of FinanceWizard.");
    }
    let counts = table_counts(&conn)?;
    export_to(&conn, target, key)?;
    conn.close().map_err(|(_, error)| error)?;
    // The last connection to close removes these, so they only remain if
    // another copy of the app has the database open.
    if sidecar(source, "-wal").exists() {
        let _ = fs::remove_file(target);
        return invalid("Close any other FinanceWizard windows and try again.");
    }
    let copy = Connection::open(target)?;
    apply_key(&copy, key)?;
    let integrity: String = copy.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" || table_counts(&copy)? != counts {
        drop(copy);
        let _ = fs::remove_file(target);
        return invalid("The encrypted copy did not match your data, so nothing was changed.");
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

#[cfg(test)]
pub mod tests {
    use super::*;

    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(name: &str) -> Self {
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

    const KEY: &str = "x'00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff'";

    /// A 0.1.0 database whose rows are still only in the `-wal` file. Returns
    /// the open connection, since closing it would move them into the main file.
    pub fn plaintext_with_wal(path: &Path) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        migrate(&conn).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('currency', 'USD')", []).unwrap();
        conn
    }

    /// Copies the files the way a killed app leaves them, with the WAL intact.
    fn copy_with_wal(path: &Path, target: &Path) {
        fs::copy(path, target).unwrap();
        fs::copy(sidecar(path, "-wal"), sidecar(target, "-wal")).unwrap();
    }

    #[test]
    fn encrypting_includes_rows_still_in_the_wal() {
        let dir = TempDir::new("encrypt-wal");
        let (live, plain, target) = (dir.0.join("live"), dir.0.join("plain.sqlite"), dir.0.join("target"));
        let _open = plaintext_with_wal(&live);
        copy_with_wal(&live, &plain);

        encrypt_plaintext(&plain, &target, KEY).unwrap();

        assert!(!is_plaintext(&target).unwrap());
        assert!(!sidecar(&plain, "-wal").exists());
        assert!(!sidecar(&plain, "-shm").exists());
        let conn = open(&target, KEY).unwrap();
        let currency: String =
            conn.query_row("SELECT value FROM settings WHERE key = 'currency'", [], |row| row.get(0)).unwrap();
        assert_eq!(currency, "USD");
    }

    #[test]
    fn a_failed_encryption_leaves_the_plaintext_usable() {
        let dir = TempDir::new("encrypt-fail");
        let plain = dir.0.join("plain.sqlite");
        drop(plaintext_with_wal(&plain));
        let target = dir.0.join("missing-folder").join("target");

        assert!(encrypt_plaintext(&plain, &target, KEY).is_err());
        assert!(is_plaintext(&plain).unwrap());
        let conn = Connection::open(&plain).unwrap();
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0)).unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn the_wrong_key_is_reported_as_such() {
        let dir = TempDir::new("wrong-key");
        let path = dir.0.join("db");
        drop(open(&path, KEY).unwrap());
        assert!(matches!(open(&path, "x'ff'"), Err(AppError::WrongKey)));
    }

    #[test]
    fn exports_keep_the_version_and_application_id() {
        let dir = TempDir::new("export");
        let conn = open(&dir.0.join("db"), KEY).unwrap();
        let copy_path = dir.0.join("copy");
        export_to(&conn, &copy_path, "a long passphrase").unwrap();

        let copy = Connection::open(&copy_path).unwrap();
        apply_key(&copy, "a long passphrase").unwrap();
        let id: i32 = copy.query_row("PRAGMA application_id", [], |row| row.get(0)).unwrap();
        assert_eq!(id, APPLICATION_ID);
        assert_eq!(user_version(&copy).unwrap(), schema_version());
    }

    #[test]
    fn opening_an_older_schema_saves_a_copy_then_upgrades_it() {
        let dir = TempDir::new("pre-update");
        let path = dir.0.join("db");
        let conn = Connection::open(&path).unwrap();
        apply_key(&conn, KEY).unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.execute("INSERT INTO categories (name, kind) VALUES ('Food', 'expense')", []).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        drop(conn);

        let conn = open(&path, KEY).unwrap();
        let color: Option<String> = conn.query_row("SELECT color FROM categories WHERE name = 'Food'", [], |row| row.get(0)).unwrap();
        assert_eq!(color, None);
        assert_eq!(user_version(&conn).unwrap(), schema_version());

        let copies: Vec<_> = fs::read_dir(dir.0.join("safety-backups")).unwrap().collect();
        assert_eq!(copies.len(), 1);
        let copy = Connection::open(copies.into_iter().next().unwrap().unwrap().path()).unwrap();
        apply_key(&copy, KEY).unwrap();
        assert_eq!(user_version(&copy).unwrap(), 1);
    }

    #[test]
    fn an_up_to_date_or_empty_database_needs_no_copy() {
        let dir = TempDir::new("no-copy");
        drop(open(&dir.0.join("db"), KEY).unwrap());
        drop(open(&dir.0.join("db"), KEY).unwrap());
        assert!(!dir.0.join("safety-backups").exists());
    }
}
