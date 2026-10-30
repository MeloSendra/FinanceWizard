# FinanceWizard

A local, single-user Windows app for recording income, expenses, recurring payments, and budgets. Your financial data never leaves your computer: the only network request is a check for new versions on GitHub. The design is described in [docs/plan.md](docs/plan.md).

## Development

You need Node.js, Rust, the Microsoft C++ Build Tools, and a native Windows Perl such as Strawberry Perl, which the bundled OpenSSL build requires. The Perl that comes with Git Bash does not work. Put yours on the `PATH` for the build.

```
npm install
npm run tauri dev      # run the app
npm run tauri build    # build the installer
cd src-tauri && cargo test
```

The installer is written to `src-tauri/target/release/bundle/nsis/`. A local `tauri build` needs the signing key below in `TAURI_SIGNING_PRIVATE_KEY`, because it also produces update files.

`tauri dev` keeps its data in a `dev` subfolder, so it never touches your real data.

## Updates

Do not uninstall to update. Version 0.1.0 has no updater, so install 0.2.0 by running its `setup.exe` over the top. From then on the app checks for a new version when it opens and offers to install it. If you ever do uninstall, leave "delete application data" unticked.

Windows SmartScreen warns about the manual installer because it is not code-signed. In-app updates are verified against the key below.

### Publishing a release

One-time setup:

1. In PowerShell, from the repository root, run `npm run tauri signer generate -- -w "$env:USERPROFILE\.tauri\financewizard.key"`. Use the full path so the private key is never created inside the repository.
2. Paste the contents of the generated `.pub` file into `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`.
3. Add the private key's contents and its password as the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
4. Keep both in a password manager. If the key is lost, installed copies can no longer update and need a manual install.

For each release, set the same version in `package.json` and `src-tauri/Cargo.toml`, commit, and merge to `master`. Then tag the merge commit on an up-to-date `master` and push the tag:

```
git checkout master
git pull
git tag v0.3.1
git push origin v0.3.1
```

Only a tag starts a release, so pushing commits does not. The release workflow stops if the tag does not match both versions. Otherwise it runs the tests, then builds, signs, and publishes the GitHub Release.

## Your data

All data is in one encrypted SQLite database at `%APPDATA%\com.financewizard.app\financewizard.sqlite`, using SQLCipher. It is kept when you update or reinstall the app.

## Encryption

The database uses a random 256-bit key. The key is stored in `vault.json` next to the database, encrypted with your PIN and then with Windows DPAPI, so it can only be opened by your Windows account on this computer. The recovery key shown at setup is the database key itself.

Protection depends on your Windows login: someone who has both your Windows password and a copy of your disk only has the PIN left to guess. Use a strong Windows password and turn on BitLocker. Encryption protects files at rest, not a computer running malware while you are logged in. Unencrypted copies made before version 0.2.0 stay unencrypted, and the deleted original may remain recoverable from the disk until it is overwritten.

| What happened | What to do |
|---|---|
| Forgot the PIN, or 10 wrong attempts | Enter the recovery key and choose a new PIN |
| Windows reinstalled, or the account password was force-reset | Copy `financewizard.sqlite` back, enter the recovery key, choose a new PIN |
| `vault.json` lost | Enter the recovery key and choose a new PIN |
| Forgot a backup's passphrase | That backup cannot be opened. The recovery key does not work on backups. |
| Lost the PIN and the recovery key | The data on this computer cannot be opened. A backup with a known passphrase still can. |

## Backup and restore

**Settings > Back up now** writes a single encrypted `.sqlite` file wherever you choose, locked with a passphrase of 12 or more characters. The passphrase is not stored, and only it can open the backup, on any computer. Keep it in a password manager, and keep the file on another drive or in cloud storage, since the app has no copy of its own outside this computer. The date of the last backup is shown in the sidebar and in Settings.

**Restore from backup** replaces all current data with the chosen file. The app first saves the current data to `%APPDATA%\com.financewizard.app\safety-backups\`, so a restore can be undone by restoring that file. It keeps a similar copy before an update changes the database. After such an update, older versions of the app cannot open the data. Safety copies use the database key and need no passphrase. Unencrypted backups from version 0.1.0 can still be restored. Files that are not FinanceWizard backups, are damaged, or come from a newer version are rejected without changing anything.

## Export

**Settings > Export transactions** writes every transaction to an unencrypted CSV file that Excel can open. Export is one-way: CSV files cannot be imported back.

## Scheduled payments

The app only runs while it is open, so scheduled payments are created when you start it, including any dates missed while it was closed. They post automatically. To correct one, edit or delete the transaction; a deleted payment does not come back.

An item with a start date in the past does not create payments for those dates by itself. They are listed under **Past payments** on the item instead, where you tick the ones to add. This opens automatically after you create such an item. For financing, dates covered by "Installments already paid" are not listed.

## Categories

Categories have two levels: parents and subcategories. A parent has a color of your choice, and its subcategories use darker shades of it.

## Appearance

**Settings > Appearance** offers System, Light, Dim, Dark, and Custom. Custom takes a background and an accent, and picks readable text colors for you. The Money remaining card can also change style: Plain, Treasure, Gold pieces, or Comparison, which counts it in something you choose, such as coffees. Treasure shows a pile based on the share of this month's income you have left, and coins fall onto it when the app opens and a scheduled payment has gone out. The falling coins follow the Windows animation setting unless you choose otherwise. These choices are stored for this Windows account rather than in the database, so backups do not include them.

## Deleting data

**Settings > Danger zone** deletes the transactions between two dates, or resets all data. Reset removes transactions, recurring items, budgets, and categories, and keeps your PIN, recovery key, and currency. Both save a copy to `safety-backups\` first, which **Restore from backup** can bring back. Those copies stay on disk, encrypted, until you delete them.

## License

[MIT](LICENSE)
