import { useEffect, useId, useRef, useState, type CSSProperties, type FormEvent, type KeyboardEvent, type ReactNode } from "react";
import { useApp } from "./state";
import { categoryColor, categoryTree, currentMonth, expandedTree, formatMonth, formatMoney, searchTree, shiftMonth } from "./format";
import { api, type Kind } from "./api";

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

/** Shown while there are no categories, which is also how a reset starts over. */
export function GetStarted({ onOwn }: { onOwn: () => void }) {
  const { refresh } = useApp();
  const { error, busy, run } = useAction();
  const useSuggested = () =>
    run(async () => {
      await api.createStarterCategories();
      refresh();
    });
  return (
    <section className="card stack">
      <div>
        <h2>Set up your categories</h2>
        <p className="hint">Every transaction goes in a category, and that is how the app sorts your money.</p>
      </div>
      <ul className="get-started">
        <li>Expense and income categories are kept apart, so each transaction only offers the ones that fit.</li>
        <li>A category can have subcategories, such as Food with Groceries inside it. The parent adds up all of them.</li>
        <li>Any expense category can hold a monthly budget, and the app warns you before it runs over.</li>
      </ul>
      <FormError message={error} />
      <div className="row">
        <button type="button" className="primary" disabled={busy} onClick={useSuggested}>
          Use suggested categories
        </button>
        <button type="button" onClick={onOwn}>
          I'll make my own
        </button>
      </div>
      <p className="hint">Suggested categories can be renamed or deleted at any time.</p>
    </section>
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

const EXPANDED_KEY = "financewizard-expanded-categories";

function loadExpanded(): number[] {
  try {
    const stored = JSON.parse(localStorage.getItem(EXPANDED_KEY) ?? "[]");
    return Array.isArray(stored) ? stored : [];
  } catch {
    return [];
  }
}

/** Parents start collapsed. The choice is remembered and shared by every tree. */
export function useExpandedCategories() {
  const [expanded, setExpanded] = useState(loadExpanded);
  const toggle = (id: number) => {
    const stored = loadExpanded();
    const next = stored.includes(id) ? stored.filter((entry) => entry !== id) : [...stored, id];
    setExpanded(next);
    try {
      localStorage.setItem(EXPANDED_KEY, JSON.stringify(next));
    } catch {
      // Losing this only means the tree collapses again next time.
    }
  };
  return { expanded, toggle };
}

export function TreeToggle({ name, open, onToggle, tabbable = true }: { name: string; open: boolean; onToggle: () => void; tabbable?: boolean }) {
  return (
    <button
      type="button"
      className="tree-toggle"
      aria-expanded={open}
      aria-label={`Subcategories of ${name}`}
      tabIndex={tabbable ? undefined : -1}
      onClick={onToggle}
    >
      <span aria-hidden="true">›</span>
    </button>
  );
}

const PICKER_GAP = 4;
const PICKER_MIN_WIDTH = 240;
const PICKER_MAX_HEIGHT = 320;
const PICKER_MIN_BELOW = 200;

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
  const { expanded, toggle } = useExpandedCategories();
  const [search, setSearch] = useState("");
  const [position, setPosition] = useState<CSSProperties>({});
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const searchBox = useRef<HTMLInputElement>(null);

  const entries = categoryTree(categories, props.kind).filter(
    ({ category }) =>
      category.id !== props.exclude &&
      (!props.topLevelOnly || category.parentId === null) &&
      (props.includeArchived || !category.archived || category.id === props.value),
  );
  const selected = categories.find((category) => category.id === props.value);
  const parents = new Set(entries.map(({ category }) => category.parentId));
  const open = [...expanded, ...(selected?.parentId ? [selected.parentId] : [])];
  const searching = search.trim() !== "";
  const shown = searching ? searchTree(entries, search) : expandedTree(entries, open);

  const pick = (value: number | null) => {
    props.onChange(value);
    panel.current?.hidePopover();
    trigger.current?.focus();
  };

  // The panel sits in the top layer, so the dialog around it can't clip it.
  const place = () => {
    const box = trigger.current?.getBoundingClientRect();
    if (!box) return;
    const below = window.innerHeight - box.bottom - PICKER_GAP * 2;
    const flip = below < PICKER_MIN_BELOW && box.top > below;
    const width = Math.max(box.width, PICKER_MIN_WIDTH);
    setPosition({
      left: Math.min(box.left, window.innerWidth - width - PICKER_GAP),
      width,
      maxHeight: Math.min(PICKER_MAX_HEIGHT, flip ? box.top - PICKER_GAP * 2 : below),
      ...(flip ? { bottom: window.innerHeight - box.top + PICKER_GAP } : { top: box.bottom + PICKER_GAP }),
    });
  };

  const moveFocus = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const options = [searchBox.current, ...(panel.current?.querySelectorAll<HTMLElement>(".picker-option") ?? [])];
    const index = options.indexOf(document.activeElement as HTMLInputElement);
    const step = event.key === "ArrowDown" ? 1 : -1;
    options[(index + step + options.length) % options.length]?.focus();
  };

  return (
    <div className="picker">
      <button
        ref={trigger}
        type="button"
        className="picker-trigger"
        popoverTarget={id}
        disabled={props.disabled}
        style={selected && ({ "--cat": categoryColor(selected, categories) } as CSSProperties)}
      >
        {selected && <i className="cat-dot" />}
        <span className={selected ? undefined : "hint"}>
          {selected ? selected.name + (selected.archived ? " (archived)" : "") : props.placeholder}
        </span>
        <span className="picker-arrow" aria-hidden="true">
          ▾
        </span>
      </button>
      {/* Field is a label, so a click in here would otherwise press the trigger. */}
      <div
        ref={panel}
        id={id}
        popover="auto"
        className="picker-panel"
        style={position}
        onClick={(event) => event.preventDefault()}
        onKeyDown={moveFocus}
        onBeforeToggle={(event) => {
          if (event.newState === "open") place();
          else setSearch("");
        }}
        onToggle={(event) => {
          if (event.newState === "open") searchBox.current?.focus();
        }}
      >
        <input
          ref={searchBox}
          aria-label="Search categories"
          placeholder="Search"
          autoComplete="off"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            if (searching && shown.length > 0) pick(shown[0].category.id);
          }}
        />
        <div className="picker-list">
          {!searching && (
            <div className="picker-row">
              <span className="tree-toggle-space" />
              <button type="button" className="picker-option" aria-pressed={props.value === null} onClick={() => pick(null)}>
                {props.placeholder}
              </button>
            </div>
          )}
          {searching && shown.length === 0 && <p className="hint">No matching categories.</p>}
          {shown.map(({ category, depth }) => (
            <div
              className="picker-row"
              key={category.id}
              style={{ "--cat": categoryColor(category, categories), paddingLeft: depth * 18 } as CSSProperties}
            >
              {!searching && parents.has(category.id) ? (
                <TreeToggle name={category.name} open={open.includes(category.id)} onToggle={() => toggle(category.id)} tabbable={false} />
              ) : (
                <span className="tree-toggle-space" />
              )}
              <button
                type="button"
                className="picker-option"
                aria-pressed={category.id === props.value}
                onClick={() => pick(category.id)}
                onKeyDown={(event) => {
                  const wanted = event.key === "ArrowRight" ? true : event.key === "ArrowLeft" ? false : null;
                  if (wanted !== null && parents.has(category.id) && open.includes(category.id) !== wanted) toggle(category.id);
                }}
              >
                <i className="cat-dot" />
                {category.name + (category.archived ? " (archived)" : "")}
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
