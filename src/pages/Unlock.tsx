import { useState } from "react";
import { api, type VaultStatus } from "../api";
import { Field, Form, FormError, useAction } from "../ui";

function PinInput(props: { value: string; onChange: (value: string) => void; autoFocus?: boolean }) {
  return (
    <input
      type="password"
      inputMode="numeric"
      autoComplete="off"
      autoFocus={props.autoFocus}
      value={props.value}
      onChange={(event) => props.onChange(event.target.value)}
    />
  );
}

function RecoveryKey({ code, onDone }: { code: string; onDone: () => void }) {
  const [saved, setSaved] = useState(false);
  return (
    <>
      <h1>Save your recovery key</h1>
      <p className="hint">
        This is the only time it is shown. It opens your data if you forget your PIN or reinstall Windows, so keep it in
        a password manager or on paper, away from this computer. Anyone who has it can read your data.
      </p>
      <code className="recovery-key">{code}</code>
      <div className="row">
        <button type="button" onClick={() => navigator.clipboard.writeText(code)}>
          Copy
        </button>
      </div>
      <label className="check">
        <input type="checkbox" checked={saved} onChange={(event) => setSaved(event.target.checked)} />
        I have saved my recovery key
      </label>
      <button type="button" className="primary" disabled={!saved} onClick={onDone}>
        Continue
      </button>
    </>
  );
}

export default function Unlock(props: { status: VaultStatus; onUnlocked: () => void }) {
  const { error, setError, busy, run } = useAction();
  const [recovering, setRecovering] = useState(props.status === "recovery");
  const [pin, setPin] = useState("");
  const [repeat, setRepeat] = useState("");
  const [code, setCode] = useState("");
  const [newCode, setNewCode] = useState<string | null>(null);

  const create = () => {
    if (pin !== repeat) return setError("The PINs do not match.");
    run(async () => setNewCode(await api.createPin(pin)));
  };

  const unlock = () =>
    run(async () => {
      try {
        await api.unlock(pin);
        props.onUnlocked();
      } catch (failure) {
        setPin("");
        if ((await api.vaultStatus()) === "recovery") setRecovering(true);
        throw failure;
      }
    });

  const recover = () => {
    if (pin !== repeat) return setError("The PINs do not match.");
    run(async () => {
      await api.recover(code, pin);
      props.onUnlocked();
    });
  };

  const showRecovery = (show: boolean) => {
    setRecovering(show);
    setError(null);
    setPin("");
    setRepeat("");
  };

  const newPinFields = (
    <>
      <Field label="New PIN" hint="6 or more digits.">
        <PinInput value={pin} onChange={setPin} autoFocus={!recovering} />
      </Field>
      <Field label="Repeat PIN">
        <PinInput value={repeat} onChange={setRepeat} />
      </Field>
    </>
  );

  let content;
  if (newCode) {
    content = <RecoveryKey code={newCode} onDone={props.onUnlocked} />;
  } else if (props.status === "setup") {
    content = (
      <>
        <h1>Choose a PIN</h1>
        <p className="hint">Your data will be encrypted on this computer, and the PIN unlocks it when the app opens.</p>
        <Form onSubmit={create}>
          {newPinFields}
          <button type="submit" className="primary" disabled={busy}>
            {busy ? "Encrypting…" : "Encrypt my data"}
          </button>
        </Form>
      </>
    );
  } else if (recovering) {
    content = (
      <>
        <h1>Use your recovery key</h1>
        <p className="hint">Enter the key you saved when you set up FinanceWizard, then choose a new PIN.</p>
        <Form onSubmit={recover}>
          <Field label="Recovery key">
            <input autoComplete="off" autoFocus value={code} onChange={(event) => setCode(event.target.value)} />
          </Field>
          {newPinFields}
          <button type="submit" className="primary" disabled={busy}>
            {busy ? "Unlocking…" : "Unlock"}
          </button>
          {props.status === "locked" && (
            <button type="button" className="ghost" onClick={() => showRecovery(false)}>
              Use my PIN instead
            </button>
          )}
        </Form>
      </>
    );
  } else {
    content = (
      <>
        <h1>Enter your PIN</h1>
        <Form onSubmit={unlock}>
          <Field label="PIN">
            <PinInput value={pin} onChange={setPin} autoFocus />
          </Field>
          <button type="submit" className="primary" disabled={busy || !pin}>
            {busy ? "Unlocking…" : "Unlock"}
          </button>
          <button type="button" className="ghost" onClick={() => showRecovery(true)}>
            Forgot your PIN?
          </button>
        </Form>
      </>
    );
  }

  return (
    <main className="unlock">
      <section className="card">
        <div className="brand">FinanceWizard</div>
        {content}
        <FormError message={error} />
      </section>
    </main>
  );
}
