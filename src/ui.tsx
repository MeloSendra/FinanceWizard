import { useEffect, useRef, useState, type CSSProperties, type FormEvent, type ReactNode } from "react";
import { useApp } from "./state";
import { categoryColor, categoryTree, currentMonth, formatMonth, formatMoney, shiftMonth } from "./format";
import type { Kind } from "./api";

export function useAction() {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (failure) {
      setError(String(failure));
    } finally {
      setBusy(false);
    }
  };
  return { error, setError, busy, run };
}

export function Money({ cents }: { cents: number }) {
  const { settings } = useApp();
  return <>{formatMoney(cents, settings.currency)}</>;
}

/** For the `--cat` variable that the category styles read. */
export function useCategoryStyle(id: number): CSSProperties | undefined {
  const { categories } = useApp();
  const category = categories.find((entry) => entry.id === id);
  return category && ({ "--cat": categoryColor(category, categories) } as CSSProperties);
}

export function CategoryChip({ id, name }: { id: number; name: string }) {
  const style = useCategoryStyle(id);
  return (
    <span className="cat-chip" style={style}>
      <i className="cat-dot" />
      {name}
    </span>
  );
}

export function Dialog({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    if (!ref.current?.open) ref.current?.showModal();
  }, []);
  return (
    <dialog
      ref={ref}
      aria-label={title}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <header className="dialog-header">
        <h2>{title}</h2>
        <button type="button" className="ghost" onClick={onClose} aria-label="Close">
          ✕
        </button>
      </header>
      {children}
    </dialog>
  );
}

export function Confirm(props: {
  title: string;
  message: ReactNode;
  confirmLabel: string;
  onConfirm: () => Promise<void>;
  onClose: () => void;
}) {
  const { error, busy, run } = useAction();
  return (
    <Dialog title={props.title} onClose={props.onClose}>
      <p>{props.message}</p>
      <FormError message={error} />
      <div className="dialog-actions">
        <span />
        <button type="button" onClick={props.onClose}>
          Cancel
        </button>
        <button type="button" className="danger" disabled={busy} onClick={() => run(props.onConfirm)}>
          {props.confirmLabel}
        </button>
      </div>
    </Dialog>
  );
}

export function FormError({ message }: { message: string | null }) {
  return message ? (
    <p className="form-error" role="alert">
      {message}
    </p>
  ) : null;
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
      {hint && <span className="hint">{hint}</span>}
    </label>
  );
}

export function Form(props: { onSubmit: () => void; children: ReactNode }) {
  const submit = (event: FormEvent) => {
    event.preventDefault();
    props.onSubmit();
  };
  return <form onSubmit={submit}>{props.children}</form>;
}

export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <strong>{title}</strong>
      {children && <p>{children}</p>}
    </div>
  );
}

export function MonthNav() {
  const { month, setMonth } = useApp();
  return (
    <div className="month-nav" role="group" aria-label="Month">
      <button type="button" aria-label="Previous month" onClick={() => setMonth(shiftMonth(month, -1))}>
        ‹
      </button>
      <span aria-live="polite">{formatMonth(month)}</span>
      <button type="button" aria-label="Next month" onClick={() => setMonth(shiftMonth(month, 1))}>
        ›
      </button>
      {month !== currentMonth() && (
        <button type="button" className="ghost" onClick={() => setMonth(currentMonth())}>
          Today
        </button>
      )}
    </div>
  );
}

/** Asks for the word `DELETE`, for actions that remove a lot at once. */
export function TypedConfirm(props: {
  title: string;
  message: ReactNode;
  confirmLabel: string;
  onConfirm: () => Promise<void>;
  onClose: () => void;
}) {
  const { error, busy, run } = useAction();
  const [typed, setTyped] = useState("");
  return (
    <Dialog title={props.title} onClose={props.onClose}>
      <p>{props.message}</p>
      <Field label="Type DELETE to confirm">
        <input autoComplete="off" autoFocus value={typed} onChange={(event) => setTyped(event.target.value)} />
      </Field>
      <FormError message={error} />
      <div className="dialog-actions">
        <span />
        <button type="button" onClick={props.onClose}>
          Cancel
        </button>
        <button type="button" className="danger" disabled={busy || typed !== "DELETE"} onClick={() => run(props.onConfirm)}>
          {props.confirmLabel}
        </button>
      </div>
    </Dialog>
  );
}

/** Archived categories are only offered when they are already selected. */
export function CategorySelect(props: {
  value: number | null;
  onChange: (id: number | null) => void;
  kind?: Kind;
  placeholder: string;
  includeArchived?: boolean;
  exclude?: number;
  topLevelOnly?: boolean;
  disabled?: boolean;
}) {
  const { categories } = useApp();
  const entries = categoryTree(categories, props.kind).filter(
    ({ category }) =>
      category.id !== props.exclude &&
      (!props.topLevelOnly || category.parentId === null) &&
      (props.includeArchived || !category.archived || category.id === props.value),
  );
  return (
    <select
      value={props.value ?? ""}
      disabled={props.disabled}
      onChange={(event) => props.onChange(event.target.value ? Number(event.target.value) : null)}
    >
      <option value="">{props.placeholder}</option>
      {entries.map(({ category, depth }) => (
        <option key={category.id} value={category.id}>
          {"   ".repeat(depth) + category.name + (category.archived ? " (archived)" : "")}
        </option>
      ))}
    </select>
  );
}
