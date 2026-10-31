import { useState } from "react";
import { api, type Category, type Kind } from "../api";
import { useApp, useLoad } from "../state";
import { amountText, categoryTree, expandedTree, formatDate, nextPaletteColor, paletteColor, parseAmount } from "../format";
import { CategoryChip, CategorySelect, Confirm, Dialog, Empty, Field, Form, FormError, GetStarted, Money, TreeToggle, useAction, useCategoryStyle, useExpandedCategories } from "../ui";

interface Draft {
  category: Category | null;
  kind: Kind;
  parentId: number | null;
}

function CategoryDialog({ draft, onClose }: { draft: Draft; onClose: () => void }) {
  const { categories, refresh } = useApp();
  const { error, setError, busy, run } = useAction();
  const { category } = draft;
  const [name, setName] = useState(category?.name ?? "");
  const [kind, setKind] = useState(draft.kind);
  const [parentId, setParentId] = useState(draft.parentId);
  const [budget, setBudget] = useState(amountText(category?.defaultBudgetCents));
  const [color, setColor] = useState(category ? (category.color ?? paletteColor(category.id)) : nextPaletteColor(categories));
  const hasChildren = (category?.childCount ?? 0) > 0;

  const save = () => {
    const defaultBudgetCents = budget.trim() ? parseAmount(budget) : null;
    if (budget.trim() && defaultBudgetCents === null) return setError("Enter a valid budget, such as 250.");
    const input = { name, kind, parentId, defaultBudgetCents, color: parentId === null ? color : null };
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
          <Field label="Parent" hint={hasChildren ? "It has subcategories, so it stays top-level." : undefined}>
            <CategorySelect
              value={parentId}
              onChange={setParentId}
              kind={kind}
              placeholder="None"
              exclude={category?.id}
              topLevelOnly
              disabled={hasChildren}
            />
          </Field>
        </div>
        {parentId === null ? (
          <Field label="Color" hint="Subcategories use a darker shade of it.">
            <input type="color" className="color-input" value={color} onChange={(event) => setColor(event.target.value)} />
          </Field>
        ) : (
          <p className="hint">Subcategories use a darker shade of their parent's color.</p>
        )}
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
  const style = useCategoryStyle(category.id);

  const setArchived = (archived: boolean) => {
    api.setCategoryArchived(category.id, archived).then(refresh).catch((error) => notify(String(error)));
  };

  return (
    <section className="card category-panel" style={style}>
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
        {!category.archived && category.parentId === null && (
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
            {transaction.description || <span className="hint">No description</span>}{" "}
            <CategoryChip id={transaction.categoryId} name={transaction.categoryName} />
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

function TreeRow(props: { category: Category; depth: number; selected: boolean; open: boolean; onToggle: () => void; onSelect: () => void }) {
  const { category, depth } = props;
  const style = useCategoryStyle(category.id);
  return (
    <div className="tree-line" style={{ paddingLeft: depth * 18 }}>
      {category.childCount > 0 ? (
        <TreeToggle name={category.name} open={props.open} onToggle={props.onToggle} />
      ) : (
        <span className="tree-toggle-space" />
      )}
      <button
        type="button"
        className={`tree-row${category.archived ? " archived" : ""}`}
        aria-pressed={props.selected}
        style={style}
        onClick={props.onSelect}
      >
        <i className="cat-dot" />
        {category.name}
        {category.childCount > 0 && !props.open && <span className="hint">{category.childCount}</span>}
        {category.archived && <span className="tag">Archived</span>}
        {category.defaultBudgetCents !== null && (
          <span className="num">
            <Money cents={category.defaultBudgetCents} />
          </span>
        )}
      </button>
    </div>
  );
}

function Tree(props: { title: string; kind: Kind; selected: number | null; onSelect: (id: number) => void }) {
  const { categories } = useApp();
  const { expanded, toggle } = useExpandedCategories();
  const entries = categoryTree(categories, props.kind);
  return (
    <section className="card">
      <h2>{props.title}</h2>
      {entries.length === 0 && <p className="hint">None yet.</p>}
      {expandedTree(entries, expanded).map(({ category, depth }) => (
        <TreeRow
          key={category.id}
          category={category}
          depth={depth}
          selected={props.selected === category.id}
          open={expanded.includes(category.id)}
          onToggle={() => toggle(category.id)}
          onSelect={() => props.onSelect(category.id)}
        />
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
        <GetStarted onOwn={() => setDraft({ category: null, kind: "expense", parentId: null })} />
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
