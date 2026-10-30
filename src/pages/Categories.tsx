import { useState } from "react";
import { api, type Category, type Kind } from "../api";
import { useApp, useLoad } from "../state";
import { amountText, categoryTree, formatDate, parseAmount } from "../format";
import { CategorySelect, Confirm, Dialog, Empty, Field, Form, FormError, Money, useAction } from "../ui";

interface Draft {
  category: Category | null;
  kind: Kind;
  parentId: number | null;
}

function CategoryDialog({ draft, onClose }: { draft: Draft; onClose: () => void }) {
  const { refresh } = useApp();
  const { error, setError, busy, run } = useAction();
  const { category } = draft;
  const [name, setName] = useState(category?.name ?? "");
  const [kind, setKind] = useState(draft.kind);
  const [parentId, setParentId] = useState(draft.parentId);
  const [budget, setBudget] = useState(amountText(category?.defaultBudgetCents));

  const save = () => {
    const defaultBudgetCents = budget.trim() ? parseAmount(budget) : null;
    if (budget.trim() && defaultBudgetCents === null) return setError("Enter a valid budget, such as 250.");
    const input = { name, kind, parentId, defaultBudgetCents };
    run(async () => {
      await (category ? api.updateCategory(category.id, input) : api.createCategory(input));
      refresh();
      onClose();
    });
  };

  return (
    <Dialog title={category ? "Edit category" : "New category"} onClose={onClose}>
      <Form onSubmit={save}>
        <Field label="Name">
          <input required autoFocus value={name} onChange={(event) => setName(event.target.value)} />
        </Field>
        <div className="form-grid">
          <Field label="Kind" hint={parentId !== null ? "Inherited from the parent." : undefined}>
            <select
              value={kind}
              disabled={parentId !== null}
              onChange={(event) => setKind(event.target.value as Kind)}
            >
              <option value="expense">Expense</option>
              <option value="income">Income</option>
            </select>
          </Field>
          <Field label="Parent">
            <CategorySelect value={parentId} onChange={setParentId} kind={kind} placeholder="None" exclude={category?.id} />
          </Field>
        </div>
        {kind === "expense" && (
          <Field label="Monthly budget" hint="Optional. Individual months can override it on the Budgets page.">
            <input inputMode="decimal" value={budget} onChange={(event) => setBudget(event.target.value)} />
          </Field>
        )}
        <FormError message={error} />
        <div className="dialog-actions">
          <span />
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={busy}>
            Save
          </button>
        </div>
      </Form>
    </Dialog>
  );
}

function CategoryDetail({ category, onEdit, onAddChild }: { category: Category; onEdit: () => void; onAddChild: () => void }) {
  const { refresh, notify } = useApp();
  const [confirming, setConfirming] = useState(false);
  const recent = useLoad(() => api.listTransactions({ categoryId: category.id }), [category.id]);
  const inUse = category.transactionCount + category.childCount + category.recurringCount > 0;

  const setArchived = (archived: boolean) => {
    api.setCategoryArchived(category.id, archived).then(refresh).catch((error) => notify(String(error)));
  };

  return (
    <section className="card">
      <h2>
        {category.name}
        {category.archived && <span className="tag">Archived</span>}
      </h2>
      <p className="hint">
        {category.kind === "income" ? "Income" : "Expense"} · {category.transactionCount} transactions ·{" "}
        {category.recurringCount} recurring items
      </p>
      {category.kind === "expense" && (
        <p>
          Monthly budget:{" "}
          {category.defaultBudgetCents === null ? "none" : <Money cents={category.defaultBudgetCents} />}
        </p>
      )}
      <div className="row" style={{ margin: "12px 0" }}>
        <button type="button" onClick={onEdit}>
          Edit
        </button>
        {!category.archived && (
          <button type="button" onClick={onAddChild}>
            Add subcategory
          </button>
        )}
        <button type="button" onClick={() => setArchived(!category.archived)}>
          {category.archived ? "Restore" : "Archive"}
        </button>
        <button
          type="button"
          className="ghost"
          disabled={inUse}
          title={inUse ? "Categories in use can only be archived." : undefined}
          onClick={() => setConfirming(true)}
        >
          Delete
        </button>
      </div>
      <h3 className="field-label">History, including subcategories</h3>
      {recent?.length === 0 && <p className="hint">No transactions yet.</p>}
      {recent?.slice(0, 8).map((transaction) => (
        <div className="status-line" key={transaction.id}>
          <span>
            {transaction.description || transaction.categoryName}
            <br />
            <span className="hint">{formatDate(transaction.date)}</span>
          </span>
          <span className="num">
            <Money cents={transaction.amountCents} />
          </span>
        </div>
      ))}
      {confirming && (
        <Confirm
          title="Delete category?"
          message={`"${category.name}" will be removed.`}
          confirmLabel="Delete"
          onClose={() => setConfirming(false)}
          onConfirm={async () => {
            await api.deleteCategory(category.id);
            setConfirming(false);
            refresh();
          }}
        />
      )}
    </section>
  );
}

function Tree(props: { title: string; kind: Kind; selected: number | null; onSelect: (id: number) => void }) {
  const { categories } = useApp();
  const entries = categoryTree(categories, props.kind);
  return (
    <section className="card">
      <h2>{props.title}</h2>
      {entries.length === 0 && <p className="hint">None yet.</p>}
      {entries.map(({ category, depth }) => (
        <button
          key={category.id}
          type="button"
          className={`tree-row${category.archived ? " archived" : ""}`}
          aria-pressed={props.selected === category.id}
          style={{ paddingLeft: 8 + depth * 18 }}
          onClick={() => props.onSelect(category.id)}
        >
          {category.name}
          {category.archived && <span className="tag">Archived</span>}
          {category.defaultBudgetCents !== null && (
            <span className="num">
              <Money cents={category.defaultBudgetCents} />
            </span>
          )}
        </button>
      ))}
    </section>
  );
}

export default function Categories() {
  const { categories } = useApp();
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const selected = categories.find((category) => category.id === selectedId);

  return (
    <div className="page">
      <header className="page-header">
        <h1>Categories</h1>
        <button
          type="button"
          className="primary"
          onClick={() => setDraft({ category: null, kind: "expense", parentId: null })}
        >
          New category
        </button>
      </header>
      {categories.length === 0 ? (
        <section className="card">
          <Empty title="No categories yet">Categories organise your transactions and hold your budgets.</Empty>
        </section>
      ) : (
        <div className="two-columns">
          <div className="stack">
            <Tree title="Expenses" kind="expense" selected={selectedId} onSelect={setSelectedId} />
            <Tree title="Income" kind="income" selected={selectedId} onSelect={setSelectedId} />
          </div>
          {selected ? (
            <CategoryDetail
              category={selected}
              onEdit={() => setDraft({ category: selected, kind: selected.kind, parentId: selected.parentId })}
              onAddChild={() => setDraft({ category: null, kind: selected.kind, parentId: selected.id })}
            />
          ) : (
            <section className="card">
              <Empty title="Select a category">Its details and history appear here.</Empty>
            </section>
          )}
        </div>
      )}
      {draft && <CategoryDialog draft={draft} onClose={() => setDraft(null)} />}
    </div>
  );
}
