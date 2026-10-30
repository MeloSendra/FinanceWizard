import { api } from "../api";
import { useApp, useLoad } from "../state";
import { TrendChart } from "../charts";
import { formatDate } from "../format";
import { Empty, Money, MonthNav } from "../ui";

function greeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return "Good morning";
  return hour < 18 ? "Good afternoon" : "Good evening";
}

export function OverBudget() {
  return (
    <span className="status over">
      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
        <path d="M6 1 11.5 11H.5Z" fill="currentColor" />
      </svg>
      Over budget
    </span>
  );
}

export default function Dashboard() {
  const { month, categories } = useApp();
  const report = useLoad(() => api.monthReport(month), [month]);
  if (!report) return null;

  const alerts = report.rows.filter((row) => row.budgetCents !== null && row.projectedCents > row.budgetCents);
  const isEmpty = report.trend.every((point) => point.incomeCents === 0 && point.expenseCents === 0);

  return (
    <div className="page">
      <header className="page-header">
        <h1>{greeting()}</h1>
        <MonthNav />
      </header>

      <section className="hero-row">
        <div className="card">
          <div className="stat-label">Spent so far</div>
          <div className="hero-value">
            <Money cents={report.spentCents} />
          </div>
        </div>
        <div className="card">
          <div className="stat-label">Projected end of month</div>
          <div className="stat-value">
            <Money cents={report.projectedCents} />
          </div>
        </div>
        <div className="card">
          <div className="stat-label">Income</div>
          <div className="stat-value">
            <Money cents={report.incomeCents} />
          </div>
        </div>
        <div className="card">
          <div className="stat-label">Budget</div>
          <div className="stat-value">{report.budgetCents === null ? "None set" : <Money cents={report.budgetCents} />}</div>
        </div>
      </section>

      <section className="card">
        <h2>Income and expenses</h2>
        {isEmpty ? (
          <Empty title="Nothing recorded yet">
            {categories.length === 0
              ? "Start by adding a category, then record your first transaction."
              : "Transactions you record will appear here."}
          </Empty>
        ) : (
          <TrendChart points={report.trend} />
        )}
      </section>

      <div className="two-columns">
        <section className="card">
          <h2>Budget alerts</h2>
          {alerts.length === 0 ? (
            <p className="hint">No category is projected to go over budget this month.</p>
          ) : (
            alerts.map((row) => (
              <div className="status-line" key={row.categoryId}>
                <span>
                  {row.name}
                  <br />
                  <span className="hint">
                    <Money cents={row.projectedCents} /> of <Money cents={row.budgetCents ?? 0} />
                  </span>
                </span>
                <OverBudget />
              </div>
            ))
          )}
        </section>
        <section className="card">
          <h2>Still due this month</h2>
          {report.upcoming.length === 0 ? (
            <p className="hint">No scheduled payments left in this month.</p>
          ) : (
            report.upcoming.map((payment) => (
              <div className="status-line" key={`${payment.recurringItemId}-${payment.date}`}>
                <span>
                  {payment.name}
                  <br />
                  <span className="hint">{formatDate(payment.date)}</span>
                </span>
                <span className="num">
                  <Money cents={payment.amountCents} />
                </span>
              </div>
            ))
          )}
        </section>
      </div>
    </div>
  );
}
