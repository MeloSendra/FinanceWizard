import { useState } from "react";
import { api } from "../api";
import { useApp } from "../state";
import { formatDate } from "../format";
import { Confirm, Field, Form, FormError, useAction } from "../ui";

export default function Settings() {
  const { settings, refresh, notify } = useApp();
  const { error, setError, busy, run } = useAction();
  const [currency, setCurrency] = useState(settings.currency);
  const [confirming, setConfirming] = useState(false);

  const saveCurrency = () =>
    run(async () => {
      await api.setCurrency(currency.trim().toUpperCase());
      refresh();
      notify("Currency updated.");
    });

  const backup = () =>
    run(async () => {
      if (await api.backupDatabase()) {
        refresh();
        notify("Backup saved.");
      }
    });

  const exportCsv = () =>
    run(async () => {
      if (await api.exportTransactions()) notify("Transactions exported.");
    });

  return (
    <div className="page">
      <header className="page-header">
        <h1>Settings</h1>
      </header>

      <section className="card">
        <h2>Backup</h2>
        <p className="hint">
          Last backup: {settings.lastBackupAt ? formatDate(settings.lastBackupAt) : "never"}. Your data lives only on this
          computer, so keep a backup somewhere else as well.
        </p>
        <div className="row" style={{ marginTop: 12 }}>
          <button type="button" className="primary" disabled={busy} onClick={backup}>
            Back up now
          </button>
          <button type="button" disabled={busy} onClick={() => setConfirming(true)}>
            Restore from backup
          </button>
        </div>
      </section>

      <section className="card">
        <h2>Export</h2>
        <p className="hint">Saves every transaction as a CSV file for use outside the app. CSV files cannot be imported back.</p>
        <div className="row" style={{ marginTop: 12 }}>
          <button type="button" disabled={busy} onClick={exportCsv}>
            Export transactions
          </button>
        </div>
      </section>

      <section className="card">
        <h2>Currency</h2>
        <Form onSubmit={saveCurrency}>
          <Field label="Currency code" hint="Three letters, such as EUR. Amounts are relabelled, not converted.">
            <input
              maxLength={3}
              style={{ width: 90 }}
              value={currency}
              onChange={(event) => setCurrency(event.target.value)}
            />
          </Field>
          <div className="row">
            <button type="submit" disabled={busy || currency === settings.currency}>
              Save currency
            </button>
          </div>
        </Form>
      </section>

      <FormError message={error} />

      {confirming && (
        <Confirm
          title="Restore from a backup?"
          message="This replaces all current data with the backup you choose. A safety copy of the current data is saved first."
          confirmLabel="Choose backup"
          onClose={() => setConfirming(false)}
          onConfirm={async () => {
            setConfirming(false);
            setError(null);
            await run(async () => {
              if (await api.restoreDatabase()) {
                await api.generateDuePayments();
                refresh();
                notify("Backup restored.");
              }
            });
          }}
        />
      )}
    </div>
  );
}
