import { useState } from "react";
import { api, type BackupChoice } from "../api";
import { useApp } from "../state";
import { formatDate } from "../format";
import { Confirm, Dialog, Field, Form, FormError, useAction } from "../ui";

function PassphraseDialog(props: {
  title: string;
  hint: string;
  submitLabel: string;
  repeat?: boolean;
  onSubmit: (passphrase: string) => Promise<void>;
  onClose: () => void;
}) {
  const { error, setError, busy, run } = useAction();
  const [passphrase, setPassphrase] = useState("");
  const [repeated, setRepeated] = useState("");

  const submit = () => {
    if (props.repeat && passphrase !== repeated) return setError("The passphrases do not match.");
    run(() => props.onSubmit(passphrase));
  };

  return (
    <Dialog title={props.title} onClose={props.onClose}>
      <Form onSubmit={submit}>
        <Field label="Passphrase" hint={props.hint}>
          <input
            type="password"
            autoComplete="off"
            autoFocus
            value={passphrase}
            onChange={(event) => setPassphrase(event.target.value)}
          />
        </Field>
        {props.repeat && (
          <Field label="Repeat passphrase">
            <input
              type="password"
              autoComplete="off"
              value={repeated}
              onChange={(event) => setRepeated(event.target.value)}
            />
          </Field>
        )}
        <FormError message={error} />
        <div className="dialog-actions">
          <span />
          <button type="button" onClick={props.onClose}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={busy || !passphrase}>
            {props.submitLabel}
          </button>
        </div>
      </Form>
    </Dialog>
  );
}

function ChangePin() {
  const { notify } = useApp();
  const { error, setError, busy, run } = useAction();
  const [current, setCurrent] = useState("");
  const [pin, setPin] = useState("");
  const [repeated, setRepeated] = useState("");

  const save = () => {
    if (pin !== repeated) return setError("The new PINs do not match.");
    run(async () => {
      await api.changePin(current, pin);
      setCurrent("");
      setPin("");
      setRepeated("");
      notify("PIN changed.");
    });
  };

  const input = (value: string, onChange: (value: string) => void) => (
    <input
      type="password"
      inputMode="numeric"
      autoComplete="off"
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  );

  return (
    <section className="card">
      <h2>PIN</h2>
      <Form onSubmit={save}>
        <Field label="Current PIN">{input(current, setCurrent)}</Field>
        <div className="form-grid">
          <Field label="New PIN" hint="6 or more digits.">
            {input(pin, setPin)}
          </Field>
          <Field label="Repeat new PIN">{input(repeated, setRepeated)}</Field>
        </div>
        <FormError message={error} />
        <div className="row">
          <button type="submit" disabled={busy || !current || !pin}>
            Change PIN
          </button>
        </div>
      </Form>
    </section>
  );
}

export default function Settings() {
  const { settings, refresh, notify } = useApp();
  const { error, setError, busy, run } = useAction();
  const [currency, setCurrency] = useState(settings.currency);
  const [confirming, setConfirming] = useState(false);
  const [backingUp, setBackingUp] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [restoring, setRestoring] = useState<BackupChoice | null>(null);

  const saveCurrency = () =>
    run(async () => {
      await api.setCurrency(currency.trim().toUpperCase());
      refresh();
      notify("Currency updated.");
    });

  const backup = async (passphrase: string) => {
    if (await api.backupDatabase(passphrase)) {
      refresh();
      notify("Backup saved.");
    }
    setBackingUp(false);
  };

  const restore = async (path: string, passphrase: string | null) => {
    await api.restoreDatabase(path, passphrase);
    setRestoring(null);
    await api.generateDuePayments();
    refresh();
    notify("Backup restored.");
  };

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
          computer, so keep a backup somewhere else as well. Backups are locked with a passphrase, and the recovery key
          cannot open them.
        </p>
        <div className="row" style={{ marginTop: 12 }}>
          <button type="button" className="primary" disabled={busy} onClick={() => setBackingUp(true)}>
            Back up now
          </button>
          <button type="button" disabled={busy} onClick={() => setConfirming(true)}>
            Restore from backup
          </button>
        </div>
      </section>

      <section className="card">
        <h2>Export</h2>
        <p className="hint">
          Saves every transaction as a CSV file for use outside the app. CSV files are not encrypted and cannot be imported
          back.
        </p>
        <div className="row" style={{ marginTop: 12 }}>
          <button type="button" disabled={busy} onClick={() => setExporting(true)}>
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

      <ChangePin />

      <FormError message={error} />

      {backingUp && (
        <PassphraseDialog
          title="Lock this backup"
          hint="12 or more characters. Without it this backup cannot be opened by anyone, so keep it in a password manager."
          submitLabel="Choose where to save"
          repeat
          onSubmit={backup}
          onClose={() => setBackingUp(false)}
        />
      )}

      {restoring && (
        <PassphraseDialog
          title="Unlock this backup"
          hint="The passphrase chosen when this backup was saved."
          submitLabel="Restore"
          onSubmit={(passphrase) => restore(restoring.path, passphrase)}
          onClose={() => setRestoring(null)}
        />
      )}

      {exporting && (
        <Confirm
          title="Export without encryption?"
          message="The CSV file is not encrypted, so anyone who gets it can read your transactions. Delete it when you are done with it."
          confirmLabel="Choose where to save"
          onClose={() => setExporting(false)}
          onConfirm={async () => {
            setExporting(false);
            await exportCsv();
          }}
        />
      )}

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
              const choice = await api.chooseBackup();
              if (!choice) return;
              if (choice.needsPassphrase) setRestoring(choice);
              else await restore(choice.path, null);
            });
          }}
        />
      )}
    </div>
  );
}
