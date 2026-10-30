use crate::backup::{self, Settings};
use crate::categories::{self, Category, CategoryInput};
use crate::error::{AppError, Result};
use crate::recurring::{self, RecurringInput, RecurringItem, ValueRecord};
use crate::reports::{self, MonthReport};
use crate::transactions::{self, Transaction, TransactionFilter, TransactionInput};
use chrono::{Local, NaiveDate};
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

pub struct Database(pub Mutex<Connection>);

impl Database {
    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[tauri::command]
pub fn list_categories(db: State<Database>) -> Result<Vec<Category>> {
    categories::list(&db.lock())
}

#[tauri::command]
pub fn create_category(db: State<Database>, input: CategoryInput) -> Result<Category> {
    categories::create(&db.lock(), &input)
}

#[tauri::command]
pub fn update_category(db: State<Database>, id: i64, input: CategoryInput) -> Result<Category> {
    categories::update(&db.lock(), id, &input)
}

#[tauri::command]
pub fn set_category_archived(db: State<Database>, id: i64, archived: bool) -> Result<Category> {
    categories::set_archived(&db.lock(), id, archived)
}

#[tauri::command]
pub fn delete_category(db: State<Database>, id: i64) -> Result<()> {
    categories::delete(&db.lock(), id)
}

#[tauri::command]
pub fn list_transactions(db: State<Database>, filter: TransactionFilter) -> Result<Vec<Transaction>> {
    transactions::list(&db.lock(), &filter)
}

#[tauri::command]
pub fn create_transaction(db: State<Database>, input: TransactionInput) -> Result<Transaction> {
    transactions::create(&db.lock(), &input)
}

#[tauri::command]
pub fn update_transaction(db: State<Database>, id: i64, input: TransactionInput) -> Result<Transaction> {
    transactions::update(&db.lock(), id, &input)
}

#[tauri::command]
pub fn delete_transaction(db: State<Database>, id: i64) -> Result<()> {
    transactions::delete(&db.lock(), id)
}

#[tauri::command]
pub fn generate_due_payments(db: State<Database>) -> Result<usize> {
    recurring::generate_due(&mut db.lock(), today())
}

#[tauri::command]
pub fn list_recurring(db: State<Database>) -> Result<Vec<RecurringItem>> {
    recurring::list(&db.lock(), today())
}

#[tauri::command]
pub fn create_recurring(db: State<Database>, input: RecurringInput, amount_cents: i64) -> Result<RecurringItem> {
    recurring::create(&mut db.lock(), &input, amount_cents, today())
}

#[tauri::command]
pub fn update_recurring(db: State<Database>, id: i64, input: RecurringInput) -> Result<RecurringItem> {
    recurring::update(&db.lock(), id, &input, today())
}

#[tauri::command]
pub fn delete_recurring(db: State<Database>, id: i64) -> Result<()> {
    recurring::delete(&db.lock(), id)
}

#[tauri::command]
pub fn list_recurring_values(db: State<Database>, item_id: i64) -> Result<Vec<ValueRecord>> {
    recurring::list_values(&db.lock(), item_id)
}

#[tauri::command]
pub fn add_recurring_value(db: State<Database>, item_id: i64, effective_date: String, amount_cents: i64) -> Result<()> {
    recurring::add_value(&db.lock(), item_id, &effective_date, amount_cents)
}

#[tauri::command]
pub fn update_recurring_value(db: State<Database>, id: i64, effective_date: String, amount_cents: i64) -> Result<()> {
    recurring::update_value(&db.lock(), id, &effective_date, amount_cents)
}

#[tauri::command]
pub fn delete_recurring_value(db: State<Database>, id: i64) -> Result<()> {
    recurring::delete_value(&db.lock(), id)
}

#[tauri::command]
pub fn month_report(db: State<Database>, month: String) -> Result<MonthReport> {
    reports::month_report(&db.lock(), &month, today())
}

#[tauri::command]
pub fn set_budget(db: State<Database>, month: String, category_id: i64, amount_cents: i64) -> Result<()> {
    reports::set_budget(&db.lock(), &month, category_id, amount_cents)
}

#[tauri::command]
pub fn clear_budget(db: State<Database>, month: String, category_id: i64) -> Result<()> {
    reports::clear_budget(&db.lock(), &month, category_id)
}

#[tauri::command]
pub fn get_settings(db: State<Database>) -> Result<Settings> {
    backup::settings(&db.lock())
}

#[tauri::command]
pub fn set_currency(db: State<Database>, currency: String) -> Result<()> {
    backup::set_currency(&db.lock(), &currency)
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
pub fn backup_database(app: AppHandle, db: State<Database>) -> Result<bool> {
    let name = format!("FinanceWizard-backup-{}.sqlite", timestamp("%Y-%m-%d"));
    let Some(path) = save_path(&app, "Save backup", &name, "FinanceWizard backup", "sqlite") else {
        return Ok(false);
    };
    backup::create_backup(&db.lock(), &path, &timestamp("%Y-%m-%dT%H:%M:%S"))?;
    Ok(true)
}

#[tauri::command(async)]
pub fn restore_database(app: AppHandle, db: State<Database>) -> Result<bool> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Choose a backup to restore")
        .add_filter("FinanceWizard backup", &["sqlite"])
        .blocking_pick_file()
        .and_then(|file| file.into_path().ok())
    else {
        return Ok(false);
    };
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::Invalid(error.to_string()))?
        .join("safety-backups");
    fs::create_dir_all(&directory)?;
    let safety = directory.join(format!("before-restore-{}.sqlite", timestamp("%Y-%m-%d-%H%M%S")));
    backup::restore_backup(&mut db.lock(), &path, &safety)?;
    Ok(true)
}

#[tauri::command(async)]
pub fn export_transactions(app: AppHandle, db: State<Database>) -> Result<bool> {
    let name = format!("FinanceWizard-transactions-{}.csv", timestamp("%Y-%m-%d"));
    let Some(path) = save_path(&app, "Export transactions", &name, "CSV file", "csv") else {
        return Ok(false);
    };
    let csv = backup::transactions_csv(&db.lock())?;
    fs::write(path, csv)?;
    Ok(true)
}
