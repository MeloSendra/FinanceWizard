mod backup;
mod categories;
mod commands;
mod dates;
mod db;
mod error;
mod purge;
mod recurring;
mod reports;
mod transactions;
mod vault;

use commands::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            // `tauri dev` shares the app identifier, and must not touch real data.
            #[cfg(debug_assertions)]
            let directory = directory.join("dev");
            std::fs::create_dir_all(&directory)?;
            app.manage(Database::new(directory));
            app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::create_pin,
            commands::unlock,
            commands::recover,
            commands::change_pin,
            commands::list_categories,
            commands::create_starter_categories,
            commands::transaction_suggestions,
            commands::create_category,
            commands::update_category,
            commands::set_category_archived,
            commands::delete_category,
            commands::list_transactions,
            commands::create_transaction,
            commands::update_transaction,
            commands::delete_transaction,
            commands::generate_due_payments,
            commands::list_recurring,
            commands::create_recurring,
            commands::update_recurring,
            commands::delete_recurring,
            commands::list_recurring_values,
            commands::add_recurring_value,
            commands::update_recurring_value,
            commands::delete_recurring_value,
            commands::missed_payments,
            commands::add_missed_payments,
            commands::month_report,
            commands::set_budget,
            commands::clear_budget,
            commands::get_settings,
            commands::set_currency,
            commands::backup_database,
            commands::choose_backup,
            commands::restore_database,
            commands::export_transactions,
            commands::count_transactions,
            commands::purge_transactions,
            commands::reset_all_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FinanceWizard");
}
