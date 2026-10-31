import { useState } from "react";
import { api, type SavingsMonth } from "../api";
import { useApp, useLoad } from "../state";
import { SavingsChart } from "../charts";
import { amountText, formatMonth, parseAmount } from "../format";
import { Empty, Field, Form, FormError, Money, useAction } from "../ui";

/** More than this and the bars get too thin to read. The table has every month. */
const CHART_MONTHS = 24;

function StartingPoint() {
  const { settings, refresh } = useApp();
  const { error, setError, busy, run } = useAction();
  const [opening, setOpening] = useState(settings.savingsOpeningCents ? amountText(settings.savingsOpeningCents) : "");
  const [startMonth, setStartMonth] = useState(settings.savingsStartMonth ?? "");

  const save = () => {
    const openingCents = opening.trim() ? parseAmount(opening) : 0;
    if (openingCents === null) return setError("Enter a valid amount, such as 1500.");
    run(async () => {
      await api.setSavings(openingCents, startMonth || null);
      refresh();
    });
  };

  return (
    <section className="card">
      <h2>Starting point</h2>
      <Form onSubmit={save}>
        <div className="form-grid">
          <Field label="Already saved" hint="What you had before these months.">
            <input inputMode="decimal" value={opening} onChange={(event) => setOpening(event.target.value)} />
          </Field>
          <Field label="Count from" hint="Leave empty to start at your first transaction.">
            <input type="month" value={startMonth} onChange={(event) => setStartMonth(event.target.value)} />
          </Field>
        </div>
        <FormError message={error} />
        <div className="row">
          <button type="submit" disabled={busy}>
            Save
          </button>
        </div>
      </Form>
    </section>
  );
}

function History({ months }: { months: SavingsMonth[] }) {
  return (
    <table>
      <thead>
        <tr>
          <th>Month</th>
          <th className="num">Income</th>
          <th className="num">Expenses</th>
          <th className="num">Saved</th>
        </tr>
      </thead>
      <tbody>
        {[...months].reverse().map((month) => (
          <tr key={month.month}>
            <td>{formatMonth(month.month)}</td>
            <td className="num">
              <Money cents={month.incomeCents} />
            </td>
            <td className="num">
              <Money cents={month.expenseCents} />
            </td>
            <td className={`num${month.savedCents < 0 ? " negative" : ""}`}>
              <Money cents={month.savedCents} />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export default function Savings() {
  const report = useLoad(() => api.savingsReport(), []);
  if (!report) return null;

  return (
    <div className="page">
      <header className="page-header">
        <h1>Savings</h1>
      </header>

      <section className="summary savings-summary">
        <div className="card">
          <div className="stat-label">Total savings</div>
          <div className={`hero-value${report.totalCents < 0 ? " negative" : ""}`}>
            <Money cents={report.totalCents} />
          </div>
        </div>
        <div className="card">
          <div className="stat-label">This month so far</div>
          <div className={`stat-value${report.currentMonthCents < 0 ? " negative" : ""}`}>
            <Money cents={report.currentMonthCents} />
          </div>
          <div className="hint">Added to the total when the month ends.</div>
        </div>
      </section>

      {report.months.length === 0 ? (
        <section className="card">
          <Empty title="No finished months yet">Each month's income minus its expenses is added here once the month ends.</Empty>
        </section>
      ) : (
        <>
          <section className="card">
            <h2>Saved each month</h2>
            <SavingsChart months={report.months.slice(-CHART_MONTHS)} />
          </section>
          <section className="card flush">
            <History months={report.months} />
          </section>
        </>
      )}

      <StartingPoint />
      <p className="hint">Savings are what was left of each month's income, and they are never part of your money remaining.</p>
    </div>
  );
}
