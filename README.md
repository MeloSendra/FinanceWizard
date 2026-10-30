# FinanceWizard

A local, single-user Windows app for recording income, expenses, recurring payments, and budgets. Nothing leaves your computer. The design is described in [docs/plan.md](docs/plan.md).

## Development

You need Node.js, Rust, and the Microsoft C++ Build Tools.

```
npm install
npm run tauri dev      # run the app
npm run tauri build    # build the installer
cd src-tauri && cargo test
```

The installer is written to `src-tauri/target/release/bundle/nsis/`.

## Your data

All data is in one SQLite database at `%APPDATA%\com.financewizard.app\financewizard.sqlite`. It is kept when you update or reinstall the app.

## Backup and restore

**Settings > Back up now** writes a single `.sqlite` file wherever you choose. Keep it on another drive or in cloud storage, since the app has no copy of its own outside this computer. The date of the last backup is shown in the sidebar and in Settings.

**Restore from backup** replaces all current data with the chosen file. The app first saves the current data to `%APPDATA%\com.financewizard.app\safety-backups\`, so a restore can be undone by restoring that file. Files that are not FinanceWizard backups, are damaged, or come from a newer version are rejected without changing anything.

## Export

**Settings > Export transactions** writes every transaction to a CSV file that Excel can open. Export is one-way: CSV files cannot be imported back.

## Scheduled payments

The app only runs while it is open, so scheduled payments are created when you start it, including any dates missed while it was closed. They post automatically. To correct one, edit or delete the transaction; a deleted payment does not come back.
