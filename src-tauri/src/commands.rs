use crate::backup::{self, Settings};
use crate::categories::{self, Category, CategoryInput, Kind};
use crate::error::{AppError, Result};
use crate::purge;
use crate::recurring::{self, MissedPayment, RecurringInput, RecurringItem, ValueRecord};
use crate::reports::{self, MonthReport, SavingsReport};
use crate::transactions::{self, Suggestion, Transaction, TransactionFilter, TransactionInput};
use crate::vault::{self, DATABASE_FILE, Key, Status};
use chrono::{Local, NaiveDate};
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

struct Session {
    conn: Connection,
    key: Key,
}

/// `session` is empty until the app has been unlocked.
pub struct Database {
    directory: PathBuf,
    session: Mutex<Option<Session>>,
}

/// An open database. Only built once the session has been checked.
pub struct Unlocked<'a>(MutexGuard<'a, Option<Session>>);

impl Unlocked<'_> {
    fn session(&mut self) -> &mut Session {
        self.0.as_mut().expect("checked by Database::lock")
    }
}

impl Deref for Unlocked<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.0.as_ref().expect("checked by Database::lock").conn
    }
}

impl DerefMut for Unlocked<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.session().conn
    }
}

impl Database {
    pub fn new(directory: PathBuf) -> Self {
        Database { directory, session: Mutex::new(None) }
    }

    fn session(&self) -> MutexGuard<'_, Option<Session>> {
        self.session.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock(&self) -> Result<Unlocked<'_>> {
        let session = self.session();
        if session.is_none() {
            return Err(AppError::Locked);
        }
        Ok(Unlocked(session))
    }

    fn start(&self, conn: Connection, key: Key) {
        *self.session() = Some(Session { conn, key });
    }
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[tauri::command]
pub fn vault_status(db: State<Database>) -> Result<Status> {
    if db.session().is_some() {
        return Ok(Status::Unlocked);
    }
    vault::status(&db.directory)
}

// These run off the main thread (`async`) because the key derivation is slow on
// purpose, and encrypting an existing database can take longer still.

/// Returns the recovery key, which is shown once and never stored in that form.
#[tauri::command(async)]
pub fn create_pin(db: State<Database>, pin: String) -> Result<String> {
    let pin = Zeroizing::new(pin);
    let (conn, key, code) = vault::create(&db.directory, &pin)?;
    db.start(conn, key);
    Ok(code.to_string())
}

#[tauri::command(async)]
pub fn unlock(db: State<Database>, pin: String) -> Result<()> {
    let pin = Zeroizing::new(pin);
    let (conn, key) = vault::unlock(&db.directory, &pin)?;
    db.start(conn, key);
    Ok(())
}

#[tauri::command(async)]
pub fn recover(db: State<Database>, code: String, pin: String) -> Result<()> {
    let (code, pin) = (Zeroizing::new(code), Zeroizing::new(pin));
    let (conn, key) = vault::recover(&db.directory, &code, &pin)?;
    db.start(conn, key);
    Ok(())
}

#[tauri::command(async)]
pub fn change_pin(db: State<Database>, current: String, new: String) -> Result<()> {
    let (current, new) = (Zeroizing::new(current), Zeroizing::new(new));
    vault::change_pin(&db.directory, &current, &new)
}

#[tauri::command]
pub fn list_categories(db: State<Database>) -> Result<Vec<Category>> {
    categories::list(&*db.lock()?)
}

#[tauri::command]
pub fn create_category(db: State<Database>, input: CategoryInput) -> Result<Category> {
    categories::create(&*db.lock()?, &input)
}

#[tauri::command]
pub fn update_category(db: State<Database>, id: i64, input: CategoryInput) -> Result<Category> {
    categories::update(&*db.lock()?, id, &input)
}

#[tauri::command]
pub fn set_category_archived(db: State<Database>, id: i64, archived: bool) -> Result<Category> {
    categories::set_archived(&*db.lock()?, id, archived)
}

#[tauri::command]
pub fn delete_category(db: State<Database>, id: i64) -> Result<()> {
    categories::delete(&*db.lock()?, id)
}

#[tauri::command]
pub fn create_starter_categories(db: State<Database>) -> Result<()> {
    categories::create_starter(&mut *db.lock()?)
}

#[tauri::command]
pub fn list_transactions(db: State<Database>, filter: TransactionFilter) -> Result<Vec<Transaction>> {
    transactions::list(&*db.lock()?, &filter)
}

#[tauri::command]
pub fn transaction_suggestions(db: State<Database>, kind: Kind) -> Result<Vec<Suggestion>> {
    transactions::suggestions(&*db.lock()?, kind)
}

#[tauri::command]
pub fn create_transaction(db: State<Database>, input: TransactionInput) -> Result<Transaction> {
    transactions::create(&*db.lock()?, &input)
}

#[tauri::command]
pub fn update_transaction(db: State<Database>, id: i64, input: TransactionInput) -> Result<Transaction> {
    transactions::update(&*db.lock()?, id, &input, today())
}

#[tauri::command]
pub fn delete_transaction(db: State<Database>, id: i64) -> Result<()> {
    transactions::delete(&*db.lock()?, id, today())
}

#[tauri::command]
pub fn generate_due_payments(db: State<Database>) -> Result<usize> {
    recurring::generate_due(&mut *db.lock()?, today())
}

#[tauri::command]
pub fn list_recurring(db: State<Database>) -> Result<Vec<RecurringItem>> {
    recurring::list(&*db.lock()?, today())
}

#[tauri::command]
pub fn create_recurring(db: State<Database>, input: RecurringInput, amount_cents: i64) -> Result<RecurringItem> {
    recurring::create(&mut *db.lock()?, &input, amount_cents, today())
}

#[tauri::command]
pub fn update_recurring(db: State<Database>, id: i64, input: RecurringInput) -> Result<RecurringItem> {
    recurring::update(&*db.lock()?, id, &input, today())
}

#[tauri::command]
pub fn delete_recurring(db: State<Database>, id: i64) -> Result<()> {
    recurring::delete(&*db.lock()?, id)
}

#[tauri::command]
pub fn list_recurring_values(db: State<Database>, item_id: i64) -> Result<Vec<ValueRecord>> {
    recurring::list_values(&*db.lock()?, item_id)
}

#[tauri::command]
pub fn add_recurring_value(db: State<Database>, item_id: i64, effective_date: String, amount_cents: i64) -> Result<()> {
    recurring::add_value(&*db.lock()?, item_id, &effective_date, amount_cents)
}

#[tauri::command]
pub fn update_recurring_value(db: State<Database>, id: i64, effective_date: String, amount_cents: i64) -> Result<()> {
    recurring::update_value(&*db.lock()?, id, &effective_date, amount_cents)
}

#[tauri::command]
pub fn delete_recurring_value(db: State<Database>, id: i64) -> Result<()> {
    recurring::delete_value(&*db.lock()?, id)
}

#[tauri::command]
pub fn missed_payments(db: State<Database>, id: i64) -> Result<Vec<MissedPayment>> {
    recurring::missed_payments(&*db.lock()?, id, today())
}

#[tauri::command]
pub fn add_missed_payments(db: State<Database>, id: i64, dates: Vec<String>) -> Result<usize> {
    recurring::add_missed_payments(&mut *db.lock()?, id, &dates, today())
}

#[tauri::command]
pub fn month_report(db: State<Database>, month: String) -> Result<MonthReport> {
    reports::month_report(&*db.lock()?, &month, today())
}

#[tauri::command]
pub fn savings_report(db: State<Database>) -> Result<SavingsReport> {
    reports::savings_report(&*db.lock()?, today())
}

#[tauri::command]
pub fn set_savings(db: State<Database>, opening_cents: i64, start_month: Option<String>) -> Result<()> {
    backup::set_savings(&*db.lock()?, opening_cents, start_month.as_deref())
}

#[tauri::command]
pub fn set_budget(db: State<Database>, month: String, category_id: i64, amount_cents: i64) -> Result<()> {
    reports::set_budget(&*db.lock()?, &month, category_id, amount_cents)
}

#[tauri::command]
pub fn clear_budget(db: State<Database>, month: String, category_id: i64) -> Result<()> {
    reports::clear_budget(&*db.lock()?, &month, category_id)
}

#[tauri::command]
pub fn get_settings(db: State<Database>) -> Result<Settings> {
    backup::settings(&*db.lock()?)
}

#[tauri::command]
pub fn set_currency(db: State<Database>, currency: String) -> Result<()> {
    backup::set_currency(&*db.lock()?, &currency)
}

fn timestamp(format: &str) -> String {
    Local::now().format(format).to_string()
}

fn save_path(app: &AppHandle, title: &str, name: &str, filter: &str, extension: &str) -> Option<PathBuf> {
    app.dialog()
        .file()
        .set_title(title)
        .set_file_name(name)
        .add_filter(filter, &[extension])
        .blocking_save_file()?
        .into_path()
        .ok()
}

// The file dialogs block, so these run off the main thread (`async`) and the
// database lock is only taken once a path has been chosen.

/// Returns false when the dialog was cancelled.
#[tauri::command(async)]
pub fn backup_database(app: AppHandle, db: State<Database>, passphrase: String) -> Result<bool> {
    let passphrase = Zeroizing::new(passphrase);
    let name = format!("FinanceWizard-backup-{}.sqlite", timestamp("%Y-%m-%d"));
    let Some(path) = save_path(&app, "Save backup", &name, "FinanceWizard backup", "sqlite") else {
        return Ok(false);
    };
    backup::create_backup(&*db.lock()?, &path, &passphrase, &timestamp("%Y-%m-%dT%H:%M:%S"))?;
    Ok(true)
}

fn safety_path(db: &Database, name: &str) -> Result<PathBuf> {
    let directory = db.directory.join("safety-backups");
    fs::create_dir_all(&directory)?;
    Ok(directory.join(format!("{name}-{}.sqlite", timestamp("%Y-%m-%d-%H%M%S"))))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupChoice {
    path: PathBuf,
    needs_passphrase: bool,
}

/// Returns nothing when the dialog was cancelled.
#[tauri::command(async)]
pub fn choose_backup(app: AppHandle, db: State<Database>) -> Result<Option<BackupChoice>> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Choose a backup to restore")
        .add_filter("FinanceWizard backup", &["sqlite"])
        .blocking_pick_file()
        .and_then(|file| file.into_path().ok())
    else {
        return Ok(None);
    };
    let mut unlocked = db.lock()?;
    let needs_passphrase = backup::needs_passphrase(&path, &unlocked.session().key.sql())?;
    Ok(Some(BackupChoice { path, needs_passphrase }))
}

#[tauri::command(async)]
pub fn restore_database(db: State<Database>, path: PathBuf, passphrase: Option<String>) -> Result<()> {
    let passphrase = passphrase.map(Zeroizing::new);
    let safety = safety_path(&db, "before-restore")?;
    let mut unlocked = db.lock()?;
    let session = unlocked.session();
    let key = session.key.sql();
    let database = db.directory.join(DATABASE_FILE);
    backup::restore_backup(&mut session.conn, &database, &key, &path, passphrase.as_ref().map(|p| p.as_str()), &safety)
}

#[tauri::command(async)]
pub fn export_transactions(app: AppHandle, db: State<Database>) -> Result<bool> {
    let name = format!("FinanceWizard-transactions-{}.csv", timestamp("%Y-%m-%d"));
    let Some(path) = save_path(&app, "Export transactions", &name, "CSV file", "csv") else {
        return Ok(false);
    };
    let csv = backup::transactions_csv(&*db.lock()?)?;
    fs::write(path, csv)?;
    Ok(true)
}

#[tauri::command]
pub fn count_transactions(db: State<Database>, from: String, to: String) -> Result<i64> {
    purge::count_transactions(&*db.lock()?, &from, &to)
}

// A purge first saves a copy that Restore from backup can bring back.

#[tauri::command(async)]
pub fn purge_transactions(db: State<Database>, from: String, to: String) -> Result<usize> {
    let safety = safety_path(&db, "before-purge")?;
    let mut unlocked = db.lock()?;
    let session = unlocked.session();
    crate::db::export_to(&session.conn, &safety, &session.key.sql())?;
    purge::delete_transactions(&session.conn, &from, &to, today())
}

#[tauri::command(async)]
pub fn reset_all_data(db: State<Database>) -> Result<()> {
    let safety = safety_path(&db, "before-reset")?;
    let mut unlocked = db.lock()?;
    let session = unlocked.session();
    crate::db::export_to(&session.conn, &safety, &session.key.sql())?;
    purge::reset_all(&mut session.conn)
}
