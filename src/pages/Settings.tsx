import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { api, type BackupChoice } from "../api";
import { useApp, useLoad } from "../state";
import { coinDropEnabled, loadCoins, saveCoins, type CoinStyle } from "../coins";
import { amountText, formatDate, parseAmount } from "../format";
import { saveTheme, loadTheme, type ThemeMode } from "../theme";
import { Confirm, Dialog, Field, Form, FormError, TypedConfirm, useAction } from "../ui";

const THEMES: { mode: ThemeMode; label: string }[] = [
  { mode: "system", label: "System" },
  { mode: "light", label: "Light" },
  { mode: "dim", label: "Dim" },
  { mode: "dark", label: "Dark" },
  { mode: "custom", label: "Custom" },
];

const COIN_STYLES: { style: CoinStyle; label: string }[] = [
  { style: "plain", label: "Plain" },
  { style: "treasure", label: "Treasure" },
  { style: "gold", label: "Gold pieces" },
  { style: "comparison", label: "Comparison" },
];

function Appearance() {
  const [theme, setTheme] = useState(loadTheme);
  const [coins, setCoins] = useState(loadCoins);
  const [price, setPrice] = useState(amountText(coins.priceCents));

  const changeCoins = (next: Partial<typeof coins>) => {
    const updated = { ...coins, ...next };
    setCoins(updated);
    saveCoins(updated);
  };

  const changePrice = (text: string) => {
    setPrice(text);
    const priceCents = parseAmount(text);
    if (priceCents) changeCoins({ priceCents });
  };

  const change = (next: Partial<typeof theme>) => {
    const updated = { ...theme, ...next };
    setTheme(updated);
    saveTheme(updated);
  };

  return (
    <section className="card">
      <h2>Appearance</h2>
      <div className="segmented" role="group" aria-label="Theme">
        {THEMES.map(({ mode, label }) => (
          <button key={mode} type="button" aria-pressed={theme.mode === mode} onClick={() => change({ mode })}>
            {label}
          </button>
        ))}
      </div>
      {theme.mode === "custom" && (
        <div className="form-grid" style={{ marginTop: 12 }}>
          <Field label="Background" hint="Text color adjusts so it stays readable.">
            <input
              type="color"
              className="color-input"
              value={theme.background}
              onChange={(event) => change({ background: event.target.value })}
            />
          </Field>
          <Field label="Accent">
            <input type="color" className="color-input" value={theme.accent} onChange={(event) => change({ accent: event.target.value })} />
          </Field>
        </div>
      )}
      <h3 className="field-label" style={{ margin: "14px 0 6px" }}>
        Money remaining
      </h3>
      <div className="segmented" role="group" aria-label="Money remaining style">
        {COIN_STYLES.map(({ style, label }) => (
          <button key={style} type="button" aria-pressed={coins.style === style} onClick={() => changeCoins({ style })}>
            {label}
          </button>
        ))}
      </div>
      {coins.style === "treasure" && (
        <label className="check" style={{ marginTop: 12 }}>
          <input
            type="checkbox"
            checked={coinDropEnabled(coins)}
            onChange={(event) => changeCoins({ coinDrop: event.target.checked })}
          />
          Drop coins when a scheduled payment goes out
        </label>
      )}
      {coins.style === "comparison" && (
        <div className="form-grid" style={{ marginTop: 12 }}>
          <Field label="Count it in" hint="In the plural, such as coffees.">
            <input value={coins.item} onChange={(event) => changeCoins({ item: event.target.value })} />
          </Field>
          <Field label="Price of one">
            <input inputMode="decimal" value={price} onChange={(event) => changePrice(event.target.value)} />
          </Field>
        </div>
      )}
    </section>
  );
}

function DangerZone() {
  const { refresh, notify } = useApp();
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [confirming, setConfirming] = useState<"range" | "reset" | null>(null);
  const ready = Boolean(from && to && from <= to);
  const count = useLoad(() => (ready ? api.countTransactions(from, to) : Promise.resolve(null)), [from, to]);

  const safety = "A safety copy is saved first, and Restore from backup can bring it back from the app's safety-backups folder.";

  return (
    <section className="card danger-zone">
      <h2>Danger zone</h2>
      <p className="hint">Delete all transactions between two dates. Recurring items and budgets stay.</p>
      <div className="form-grid" style={{ marginTop: 12 }}>
        <Field label="From">
          <input type="date" value={from} onChange={(event) => setFrom(event.target.value)} />
        </Field>
        <Field label="To">
          <input type="date" min={from || undefined} value={to} onChange={(event) => setTo(event.target.value)} />
        </Field>
      </div>
      <div className="row" style={{ marginTop: 12 }}>
        <button type="button" disabled={!ready || !count} onClick={() => setConfirming("range")}>
          {ready && count !== null ? `Delete ${count} ${count === 1 ? "transaction" : "transactions"}` : "Delete transactions"}
        </button>
        <button type="button" className="danger" onClick={() => setConfirming("reset")}>
          Reset all data
        </button>
      </div>
      {confirming === "range" && (
        <TypedConfirm
          title={`Delete ${count} ${count === 1 ? "transaction" : "transactions"}?`}
          message={`Everything from ${formatDate(from)} to ${formatDate(to)} is removed, financing progress goes down to match, and those months leave your savings history. Deleted scheduled payments move to Past payments on their item. ${safety}`}
          confirmLabel="Delete"
          onClose={() => setConfirming(null)}
          onConfirm={async () => {
            await api.purgeTransactions(from, to);
            setConfirming(null);
            setFrom("");
            setTo("");
            refresh();
            notify("Transactions deleted.");
          }}
        />
      )}
      {confirming === "reset" && (
        <TypedConfirm
          title="Reset all data?"
          message={`Every transaction, recurring item, budget and category is removed. Your PIN, recovery key and currency stay. ${safety}`}
          confirmLabel="Reset"
          onClose={() => setConfirming(null)}
          onConfirm={async () => {
            await api.resetAllData();
            setConfirming(null);
            refresh();
            notify("All data removed.");
          }}
        />
      )}
    </section>
  );
}

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

function Updates() {
  const { notify, setUpdate } = useApp();
  const { error, busy, run } = useAction();
  const [appVersion, setAppVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setAppVersion);
  }, []);

  const checkNow = () =>
    run(async () => {
      const update = await check();
      setUpdate(update);
      if (!update) notify("You have the latest version.");
    });

  return (
    <section className="card">
      <h2>Updates</h2>
      <p className="hint">
        You are using version {appVersion ?? "…"}. The app checks GitHub for new versions when it opens. No financial data
        is sent.
      </p>
      <FormError message={error} />
      <div className="row" style={{ marginTop: 12 }}>
        <button type="button" disabled={busy} onClick={checkNow}>
          Check for updates
        </button>
      </div>
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

      <Appearance />

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
      <Updates />
      <DangerZone />

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
