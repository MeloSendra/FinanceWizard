import { useState } from "react";
import { api, type BudgetRow } from "../api";
import { useApp, useLoad } from "../state";
import { CategoryDonut } from "../charts";
import { amountText, parseAmount } from "../format";
import { CategoryChip, Empty, Money, MonthNav } from "../ui";
import { OverBudget } from "./Dashboard";

/** Red once the amount passes the budget. */
function Against({ cents, budgetCents }: { cents: number; budgetCents: number | null }) {
  return (
    <td className={`num${budgetCents !== null && cents > budgetCents ? " negative" : ""}`}>
      <Money cents={cents} />
    </td>
  );
}

function Meter({ row }: { row: BudgetRow }) {
  const scale = Math.max(row.budgetCents ?? 0, row.projectedCents, 1);
  const over = row.budgetCents !== null && row.projectedCents > row.budgetCents;
  return (
    <div className="meter" aria-hidden="true">
      <span className={over ? "over" : undefined} style={{ width: `${(row.spentCents / scale) * 100}%` }} />
      {row.upcomingCents > 0 && (
        <span className={`upcoming${over ? " over" : ""}`} style={{ width: `${(row.upcomingCents / scale) * 100}%` }} />
      )}
    </div>
  );
}

function BudgetInput({ row, month }: { row: BudgetRow; month: string }) {
  const { refresh, notify } = useApp();
  const [text, setText] = useState(amountText(row.budgetCents));

  const save = () => {
    if (text === amountText(row.budgetCents)) return;
    const cents = text.trim() ? parseAmount(text) : null;
    if (cents === null) {
      setText(amountText(row.budgetCents));
      if (text.trim()) notify("Enter a valid budget, such as 250.");
      return;
    }
    api.setBudget(month, row.categoryId, cents).then(refresh).catch((error) => notify(String(error)));
  };

  return (
    <input
      className="budget-input"
      inputMode="decimal"
      aria-label={`Budget for ${row.name}`}
      value={text}
      onChange={(event) => setText(event.target.value)}
      onBlur={save}
      onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
    />
  );
}

export default function Budgets() {
  const { month, refresh, notify } = useApp();
  const report = useLoad(() => api.monthReport(month), [month]);

  const reset = (row: BudgetRow) => {
    api.clearBudget(month, row.categoryId).then(refresh).catch((error) => notify(String(error)));
  };

  return (
    <div className="page">
      <header className="page-header">
        <h1>Budgets</h1>
        <MonthNav />
      </header>
      {report && report.spentCents > 0 && (
        <section className="card">
          <h2>Where the money went</h2>
          <CategoryDonut rows={report.rows} />
        </section>
      )}
      <section className="card flush">
        {report?.rows.length === 0 ? (
          <Empty title="No expense categories yet">Add a category to start planning.</Empty>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Category</th>
                <th className="num">Budget</th>
                <th className="num">Spent so far</th>
                <th className="num">Projected</th>
                <th className="meter-column" style={{ width: "22%" }} />
              </tr>
            </thead>
            <tbody>
              {report?.rows.map((row) => (
                <tr key={row.categoryId}>
                  <td style={{ paddingLeft: 14 + row.depth * 18 }}>
                    <CategoryChip id={row.categoryId} name={row.name} />
                    {row.budgetCents !== null && row.projectedCents > row.budgetCents && (
                      <>
                        <br />
                        <OverBudget />
                      </>
                    )}
                  </td>
                  <td className="num">
                    <BudgetInput key={`${month}-${row.budgetCents}`} row={row} month={month} />
                    {row.hasOverride && (
                      <>
                        <br />
                        <button type="button" className="ghost small" onClick={() => reset(row)}>
                          Use default
                        </button>
                      </>
                    )}
                  </td>
                  <Against cents={row.spentCents} budgetCents={row.budgetCents} />
                  <Against cents={row.projectedCents} budgetCents={row.budgetCents} />
                  <td className="meter-column">{row.budgetCents !== null && <Meter row={row} />}</td>
                </tr>
              ))}
            </tbody>
            {report && (
              <tfoot>
                <tr>
                  <td>Total</td>
                  <td className="num">{report.budgetCents === null ? "–" : <Money cents={report.budgetCents} />}</td>
                  <Against cents={report.spentCents} budgetCents={report.budgetCents} />
                  <Against cents={report.projectedCents} budgetCents={report.budgetCents} />
                  <td className="meter-column" />
                </tr>
              </tfoot>
            )}
          </table>
        )}
      </section>
      <p className="hint">Budgets you enter here apply to this month only. Categories keep their default on the Categories page.</p>
    </div>
  );
}
