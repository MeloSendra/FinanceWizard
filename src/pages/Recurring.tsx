import { useState } from "react";
import { api, type Frequency, type ItemType, type RecurringItem, type Status, type ValueRecord } from "../api";
import { useApp, useLoad } from "../state";
import { ValueChart } from "../charts";
import { amountText, formatDate, formatFigure, parseAmount, todayIso } from "../format";
import { CategorySelect, Confirm, Dialog, Empty, Field, Form, FormError, Money, useAction } from "../ui";

const FREQUENCY: Record<Frequency, string> = { monthly: "per month", quarterly: "per quarter", yearly: "per year" };

function InstallmentBar({ paid, count }: { paid: number; count: number }) {
  const percent = Math.min(100, (paid / count) * 100);
  const label = `${paid} of ${count}`;
  return (
    <div className="progress" tabIndex={0} role="img" aria-label={`${label} installments paid`}>
      <div className="progress-track">
        <div className="progress-fill" style={{ width: `${percent}%` }} />
        {[25, 50, 75].map((mark) => (
          <span key={mark} className={`progress-mark${percent >= mark ? " filled" : ""}`} style={{ left: `${mark}%` }} />
        ))}
      </div>
      <span className="progress-tip">{label}</span>
    </div>
  );
}

function ItemCard({ item, onOpen }: { item: RecurringItem; onOpen: () => void }) {
  return (
    <div className={`card item-card${item.status === "completed" ? " completed" : ""}`}>
      <div className="item-title">
        <button type="button" className="ghost small" style={{ padding: 0, fontWeight: 600, color: "var(--ink)" }} onClick={onOpen}>
          {item.name}
        </button>
        <span className="num">
          <Money cents={item.currentAmountCents} /> <span className="hint">{FREQUENCY[item.frequency]}</span>
        </span>
      </div>
      {item.itemType === "financing" && item.installmentCount !== null && item.totalCents !== null && (
        <div className="progress-line">
          <InstallmentBar paid={item.paidInstallments} count={item.installmentCount} />
          <span className="num hint">
            {formatFigure(item.paidCents)} / {formatFigure(item.totalCents)}
          </span>
        </div>
      )}
      <div className="meta">
        {item.categoryName} ·{" "}
        {item.status === "completed"
          ? "Completed"
          : item.nextPaymentDate
            ? `Next payment ${formatDate(item.nextPaymentDate)}`
            : "No schedule"}
      </div>
    </div>
  );
}

function ValueHistory({ item }: { item: RecurringItem }) {
  const { refresh } = useApp();
  const { error, setError, busy, run } = useAction();
  const values = useLoad(() => api.listRecurringValues(item.id), [item.id]);
  const [editing, setEditing] = useState<ValueRecord | null>(null);
  const [date, setDate] = useState(todayIso());
  const [amount, setAmount] = useState("");
  const [removing, setRemoving] = useState<ValueRecord | null>(null);

  const startEdit = (value: ValueRecord) => {
    setEditing(value);
    setDate(value.effectiveDate);
    setAmount(amountText(value.amountCents));
  };

  const reset = () => {
    setEditing(null);
    setDate(todayIso());
    setAmount("");
  };

  const save = () => {
    const amountCents = parseAmount(amount);
    if (amountCents === null) return setError("Enter a valid amount, such as 510.");
    run(async () => {
      await (editing
        ? api.updateRecurringValue(editing.id, date, amountCents)
        : api.addRecurringValue(item.id, date, amountCents));
      reset();
      refresh();
    });
  };

  return (
    <section className="stack">
      <h2>Value history</h2>
      {values && values.length > 1 && <ValueChart values={values} />}
      <table>
        <thead>
          <tr>
            <th>Effective from</th>
            <th className="num">Amount</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {values?.map((value) => (
            <tr key={value.id}>
              <td>{formatDate(value.effectiveDate)}</td>
              <td className="num">
                <Money cents={value.amountCents} />
              </td>
              <td className="num">
                <button type="button" className="ghost small" onClick={() => startEdit(value)}>
                  Edit
                </button>
                <button type="button" className="ghost small" onClick={() => setRemoving(value)}>
                  Delete
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <Form onSubmit={save}>
        <div className="form-grid">
          <Field label={editing ? "Effective from" : "New value from"}>
            <input type="date" required value={date} onChange={(event) => setDate(event.target.value)} />
          </Field>
          <Field label="Amount">
            <input inputMode="decimal" required value={amount} onChange={(event) => setAmount(event.target.value)} />
          </Field>
        </div>
        <FormError message={error} />
        <div className="row">
          <button type="submit" disabled={busy}>
            {editing ? "Save value" : "Add value"}
          </button>
          {editing && (
            <button type="button" className="ghost" onClick={reset}>
              Cancel
            </button>
          )}
        </div>
      </Form>
      {removing && (
        <Confirm
          title="Delete this value?"
          message={`The value from ${formatDate(removing.effectiveDate)} will be removed. Existing transactions keep their amounts.`}
          confirmLabel="Delete"
          onClose={() => setRemoving(null)}
          onConfirm={async () => {
            await api.deleteRecurringValue(removing.id);
            setRemoving(null);
            refresh();
          }}
        />
      )}
    </section>
  );
}

function ItemDialog({ item, itemType, onClose }: { item: RecurringItem | null; itemType: ItemType; onClose: () => void }) {
  const { refresh } = useApp();
  const { error, setError, busy, run } = useAction();
  const [name, setName] = useState(item?.name ?? "");
  const [categoryId, setCategoryId] = useState(item?.categoryId ?? null);
  const [frequency, setFrequency] = useState<Frequency>(item?.frequency ?? "monthly");
  const [startDate, setStartDate] = useState(item?.startDate ?? todayIso());
  const [endDate, setEndDate] = useState(item?.endDate ?? "");
  const [status, setStatus] = useState<Status>(item?.status ?? "active");
  const [amount, setAmount] = useState("");
  const [scheduled, setScheduled] = useState(item ? item.scheduleDay !== null : true);
  const [scheduleDay, setScheduleDay] = useState(String(item?.scheduleDay ?? 1));
  const [total, setTotal] = useState(amountText(item?.totalCents));
  const [installments, setInstallments] = useState(String(item?.installmentCount ?? ""));
  const [openingPaid, setOpeningPaid] = useState(item?.openingPaidCents ? amountText(item.openingPaidCents) : "");
  const [openingInstallments, setOpeningInstallments] = useState(String(item?.openingPaidInstallments || ""));
  const [confirming, setConfirming] = useState(false);
  const financing = itemType === "financing";

  const save = () => {
    if (categoryId === null) return setError("Choose a category.");
    const amountCents = item ? 0 : parseAmount(amount);
    if (amountCents === null) return setError("Enter a valid amount, such as 12.99.");
    const totalCents = financing ? parseAmount(total) : null;
    if (financing && totalCents === null) return setError("Enter the total amount payable.");
    const openingPaidCents = openingPaid.trim() ? parseAmount(openingPaid) : 0;
    if (openingPaidCents === null) return setError("Enter a valid amount already paid.");
    const input = {
      name,
      itemType,
      frequency,
      startDate,
      endDate: endDate || null,
      status,
      categoryId,
      scheduleDay: scheduled ? Number(scheduleDay) : null,
      totalCents,
      installmentCount: financing ? Number(installments) : null,
      openingPaidCents,
      openingPaidInstallments: Number(openingInstallments || 0),
    };
    run(async () => {
      await (item ? api.updateRecurring(item.id, input) : api.createRecurring(input, amountCents));
      await api.generateDuePayments();
      refresh();
      onClose();
    });
  };

  return (
    <Dialog title={`${item ? "Edit" : "New"} ${financing ? "financing" : "subscription"}`} onClose={onClose}>
      <Form onSubmit={save}>
        <Field label="Name">
          <input required autoFocus value={name} onChange={(event) => setName(event.target.value)} />
        </Field>
        <div className="form-grid">
          <Field label="Category">
            <CategorySelect value={categoryId} onChange={setCategoryId} kind="expense" placeholder="Choose a category" />
          </Field>
          <Field label="Frequency">
            <select value={frequency} onChange={(event) => setFrequency(event.target.value as Frequency)}>
              <option value="monthly">Monthly</option>
              <option value="quarterly">Quarterly</option>
              <option value="yearly">Yearly</option>
            </select>
          </Field>
          <Field label="Start date">
            <input type="date" required value={startDate} onChange={(event) => setStartDate(event.target.value)} />
          </Field>
          <Field label="End date" hint="Optional.">
            <input type="date" value={endDate} onChange={(event) => setEndDate(event.target.value)} />
          </Field>
        </div>
        {!item && (
          <Field label={financing ? "Installment amount" : "Amount"} hint="Later changes go in the value history.">
            <input inputMode="decimal" required value={amount} onChange={(event) => setAmount(event.target.value)} />
          </Field>
        )}
        <label className="check">
          <input type="checkbox" checked={scheduled} onChange={(event) => setScheduled(event.target.checked)} />
          Create payments automatically
        </label>
        {scheduled && (
          <Field label="Payment day" hint="Short months use their last day.">
            <input type="number" min={1} max={31} required value={scheduleDay} onChange={(event) => setScheduleDay(event.target.value)} />
          </Field>
        )}
        {financing && (
          <>
            <div className="form-grid">
              <Field label="Total payable" hint="Including interest.">
                <input inputMode="decimal" required value={total} onChange={(event) => setTotal(event.target.value)} />
              </Field>
              <Field label="Number of installments">
                <input type="number" min={1} required value={installments} onChange={(event) => setInstallments(event.target.value)} />
              </Field>
              <Field label="Already paid" hint="Down payment or earlier payments. Creates no transaction.">
                <input inputMode="decimal" value={openingPaid} onChange={(event) => setOpeningPaid(event.target.value)} />
              </Field>
              <Field label="Installments already paid">
                <input type="number" min={0} value={openingInstallments} onChange={(event) => setOpeningInstallments(event.target.value)} />
              </Field>
            </div>
          </>
        )}
        {item && (
          <Field label="Status">
            <select value={status} onChange={(event) => setStatus(event.target.value as Status)}>
              <option value="active">Active</option>
              <option value="completed">Completed</option>
            </select>
          </Field>
        )}
        <FormError message={error} />
        <div className="dialog-actions">
          {item ? (
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
      {item && <ValueHistory item={item} />}
      {confirming && item && (
        <Confirm
          title="Delete recurring item?"
          message={`"${item.name}" and its value history will be removed. Its past transactions stay as ordinary transactions.`}
          confirmLabel="Delete"
          onClose={() => setConfirming(false)}
          onConfirm={async () => {
            await api.deleteRecurring(item.id);
            refresh();
            onClose();
          }}
        />
      )}
    </Dialog>
  );
}

function Section(props: { title: string; items: RecurringItem[]; onOpen: (item: RecurringItem) => void }) {
  return (
    <section className="stack">
      <h2>{props.title}</h2>
      {props.items.length === 0 ? (
        <p className="hint">None yet.</p>
      ) : (
        <div className="item-grid">
          {props.items.map((item) => (
            <ItemCard key={item.id} item={item} onOpen={() => props.onOpen(item)} />
          ))}
        </div>
      )}
    </section>
  );
}

export default function Recurring() {
  const { categories } = useApp();
  const items = useLoad(() => api.listRecurring(), []);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [creating, setCreating] = useState<ItemType | null>(null);
  const editing = items?.find((item) => item.id === editingId) ?? null;
  const noCategories = !categories.some((category) => category.kind === "expense");
  const open = (item: RecurringItem) => setEditingId(item.id);

  return (
    <div className="page">
      <header className="page-header">
        <h1>Recurring</h1>
        <div className="row">
          <button type="button" disabled={noCategories} onClick={() => setCreating("subscription")}>
            New subscription
          </button>
          <button type="button" className="primary" disabled={noCategories} onClick={() => setCreating("financing")}>
            New financing
          </button>
        </div>
      </header>
      {noCategories && (
        <section className="card">
          <Empty title="Add an expense category first">Recurring items post their payments to a category.</Empty>
        </section>
      )}
      <Section title="Financing" items={items?.filter((item) => item.itemType === "financing") ?? []} onOpen={open} />
      <Section title="Subscriptions" items={items?.filter((item) => item.itemType === "subscription") ?? []} onOpen={open} />
      {creating && <ItemDialog item={null} itemType={creating} onClose={() => setCreating(null)} />}
      {editing && <ItemDialog key={editing.id} item={editing} itemType={editing.itemType} onClose={() => setEditingId(null)} />}
    </div>
  );
}
