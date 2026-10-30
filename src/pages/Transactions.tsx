import { useState } from "react";
import { api, type Kind, type Transaction } from "../api";
import { useApp, useLoad } from "../state";
import { WeekChart } from "../charts";
import { addDays, amountText, formatDate, formatDayMonth, monthBounds, parseAmount, startOfWeek, todayIso, weekForMonth } from "../format";
import { CategoryChip, CategorySelect, Confirm, Dialog, Empty, Field, Form, FormError, Money, MonthNav, useAction } from "../ui";

function TransactionDialog({ transaction, onClose }: { transaction: Transaction | null; onClose: () => void }) {
  const { categories, refresh } = useApp();
  const { error, setError, busy, run } = useAction();
  const recurring = useLoad(() => api.listRecurring(), []);
  const [kind, setKind] = useState<Kind>(transaction?.kind ?? "expense");
  const [date, setDate] = useState(transaction?.date ?? todayIso());
  const [amount, setAmount] = useState(amountText(transaction?.amountCents));
  const [description, setDescription] = useState(transaction?.description ?? "");
  const [categoryId, setCategoryId] = useState(transaction?.categoryId ?? null);
  const [recurringItemId, setRecurringItemId] = useState(transaction?.recurringItemId ?? null);
  const [confirming, setConfirming] = useState(false);

  const changeKind = (next: Kind) => {
    setKind(next);
    setCategoryId(null);
    setRecurringItemId(null);
  };

  const save = () => {
    const amountCents = parseAmount(amount);
    if (amountCents === null) return setError("Enter a valid amount, such as 12.50.");
    if (categoryId === null) return setError("Choose a category.");
    const input = { date, amountCents, description, categoryId, recurringItemId };
    run(async () => {
      await (transaction ? api.updateTransaction(transaction.id, input) : api.createTransaction(input));
      refresh();
      onClose();
    });
  };

  return (
    <Dialog title={transaction ? "Edit transaction" : "New transaction"} onClose={onClose}>
      <Form onSubmit={save}>
        <div className="segmented" role="group" aria-label="Type">
          {(["expense", "income"] as const).map((option) => (
            <button key={option} type="button" aria-pressed={kind === option} onClick={() => changeKind(option)}>
              {option === "expense" ? "Expense" : "Income"}
            </button>
          ))}
        </div>
        <div className="form-grid">
          <Field label="Date">
            <input type="date" required value={date} onChange={(event) => setDate(event.target.value)} />
          </Field>
          <Field label="Amount">
            <input inputMode="decimal" required autoFocus value={amount} onChange={(event) => setAmount(event.target.value)} />
          </Field>
        </div>
        <Field label="Description">
          <input value={description} onChange={(event) => setDescription(event.target.value)} />
        </Field>
        <Field label="Category">
          <CategorySelect
            value={categoryId}
            onChange={setCategoryId}
            kind={kind}
            placeholder={categories.some((category) => category.kind === kind) ? "Choose a category" : `No ${kind} categories yet`}
          />
        </Field>
        {kind === "expense" && (
          <Field label="Payment for a recurring item" hint="Linked payments count toward financing progress.">
            <select
              value={recurringItemId ?? ""}
              onChange={(event) => setRecurringItemId(event.target.value ? Number(event.target.value) : null)}
            >
              <option value="">None</option>
              {recurring?.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </select>
          </Field>
        )}
        <FormError message={error} />
        <div className="dialog-actions">
          {transaction ? (
            <button type="button" className="ghost" onClick={() => setConfirming(true)}>
              Delete
            </button>
          ) : (
            <span />
          )}
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={busy}>
            Save
          </button>
        </div>
      </Form>
      {confirming && transaction && (
        <Confirm
          title="Delete transaction?"
          message="This removes it from all summaries and cannot be undone."
          confirmLabel="Delete"
          onClose={() => setConfirming(false)}
          onConfirm={async () => {
            await api.deleteTransaction(transaction.id);
            refresh();
            onClose();
          }}
        />
      )}
    </Dialog>
  );
}

function Summary({ transactions }: { transactions: Transaction[] }) {
  const sum = (kind: Kind) =>
    transactions.filter((entry) => entry.kind === kind).reduce((total, entry) => total + entry.amountCents, 0);
  const income = sum("income");
  const expenses = sum("expense");
  const net = income - expenses;
  return (
    <section className="summary" aria-label="Totals for these transactions">
      <div className="card">
        <div className="stat-label">Income</div>
        <div className="stat-value income-text">
          +<Money cents={income} />
        </div>
      </div>
      <div className="card">
        <div className="stat-label">Expenses</div>
        <div className="stat-value expense-text">
          −<Money cents={expenses} />
        </div>
      </div>
      <div className="card">
        <div className="stat-label">Net</div>
        <div className={`stat-value ${net < 0 ? "expense-text" : "income-text"}`}>
          <Money cents={net} />
        </div>
      </div>
    </section>
  );
}

function WeekSpending({ week, setWeek, categoryId }: { week: string; setWeek: (week: string) => void; categoryId: number | null }) {
  const last = addDays(week, 6);
  const expenses = useLoad(() => api.listTransactions({ from: week, to: last, kind: "expense", categoryId }), [week, categoryId]);
  if (!expenses) return null;
  const days = Array.from({ length: 7 }, (_, index) => {
    const date = addDays(week, index);
    const cents = expenses.filter((entry) => entry.date === date).reduce((total, entry) => total + entry.amountCents, 0);
    return { date, cents };
  });
  return (
    <section className="card">
      <div className="chart-head">
        <h2>Spending by day</h2>
        <div className="month-nav" role="group" aria-label="Week">
          <button type="button" aria-label="Previous week" onClick={() => setWeek(addDays(week, -7))}>
            ‹
          </button>
          <span aria-live="polite">
            {formatDayMonth(week)} – {formatDayMonth(last)}
          </span>
          <button type="button" aria-label="Next week" onClick={() => setWeek(addDays(week, 7))}>
            ›
          </button>
          {week !== startOfWeek(todayIso()) && (
            <button type="button" className="ghost" onClick={() => setWeek(startOfWeek(todayIso()))}>
              This week
            </button>
          )}
        </div>
      </div>
      <WeekChart days={days} />
    </section>
  );
}

export default function Transactions() {
  const { month, categories } = useApp();
  const bounds = monthBounds(month);
  const [kind, setKind] = useState<Kind | null>(null);
  const [categoryId, setCategoryId] = useState<number | null>(null);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [editing, setEditing] = useState<Transaction | "new" | null>(null);
  const [week, setWeek] = useState(weekForMonth(month));

  // The day filters narrow the selected month and reset when it changes.
  const [filterMonth, setFilterMonth] = useState(month);
  if (filterMonth !== month) {
    setFilterMonth(month);
    setFrom("");
    setTo("");
    setWeek(weekForMonth(month));
  }

  const transactions = useLoad(
    () => api.listTransactions({ from: from || bounds.from, to: to || bounds.to, kind, categoryId }),
    [month, from, to, kind, categoryId],
  );

  return (
    <div className="page">
      <header className="page-header">
        <h1>Transactions</h1>
        <div className="row">
          <MonthNav />
          <button type="button" className="primary" onClick={() => setEditing("new")}>
            New transaction
          </button>
        </div>
      </header>

      <section className="filters" aria-label="Filters">
        <Field label="Type">
          <select value={kind ?? ""} onChange={(event) => setKind((event.target.value || null) as Kind | null)}>
            <option value="">All</option>
            <option value="expense">Expense</option>
            <option value="income">Income</option>
          </select>
        </Field>
        <Field label="Category">
          <CategorySelect value={categoryId} onChange={setCategoryId} placeholder="All" includeArchived kind={kind ?? undefined} />
        </Field>
        <Field label="From">
          <input type="date" min={bounds.from} max={bounds.to} value={from} onChange={(event) => setFrom(event.target.value)} />
        </Field>
        <Field label="To">
          <input type="date" min={bounds.from} max={bounds.to} value={to} onChange={(event) => setTo(event.target.value)} />
        </Field>
      </section>

      {transactions && <Summary transactions={transactions} />}
      <WeekSpending week={week} setWeek={setWeek} categoryId={categoryId} />

      <section className="card flush">
        {transactions?.length === 0 ? (
          <Empty title="No transactions here">
            {categories.length === 0 ? "Add a category first, then record a transaction." : "Try another month or clear the filters."}
          </Empty>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Date</th>
                <th>Description</th>
                <th>Category</th>
                <th className="num">Amount</th>
              </tr>
            </thead>
            <tbody>
              {transactions?.map((transaction) => (
                <tr
                  key={transaction.id}
                  className="clickable"
                  tabIndex={0}
                  onClick={() => setEditing(transaction)}
                  onKeyDown={(event) => event.key === "Enter" && setEditing(transaction)}
                >
                  <td className="nowrap">{formatDate(transaction.date)}</td>
                  <td>
                    {transaction.description || <span className="hint">No description</span>}
                    {transaction.recurringItemName && <span className="tag">{transaction.recurringItemName}</span>}
                  </td>
                  <td>
                    <CategoryChip id={transaction.categoryId} name={transaction.categoryName} />
                  </td>
                  <td className={`num ${transaction.kind === "income" ? "income-text" : "expense-text"}`}>
                    {transaction.kind === "income" ? "+" : "−"}
                    <Money cents={transaction.amountCents} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      {editing && (
        <TransactionDialog transaction={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      )}
    </div>
  );
}
