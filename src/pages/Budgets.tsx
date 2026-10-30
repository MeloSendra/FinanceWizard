import { useState } from "react";
import { api, type BudgetRow } from "../api";
import { useApp, useLoad } from "../state";
import { amountText, parseAmount } from "../format";
import { Empty, Money, MonthNav } from "../ui";
import { OverBudget } from "./Dashboard";

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
                    {row.name}
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
                  <td className="num">
                    <Money cents={row.spentCents} />
                  </td>
                  <td className="num">
                    <Money cents={row.projectedCents} />
                  </td>
                  <td className="meter-column">{row.budgetCents !== null && <Meter row={row} />}</td>
                </tr>
              ))}
            </tbody>
            {report && (
              <tfoot>
                <tr>
                  <td>Total</td>
                  <td className="num">{report.budgetCents === null ? "–" : <Money cents={report.budgetCents} />}</td>
                  <td className="num">
                    <Money cents={report.spentCents} />
                  </td>
                  <td className="num">
                    <Money cents={report.projectedCents} />
                  </td>
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
