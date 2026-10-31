import { useState } from "react";
import { api, type MonthReport } from "../api";
import { coinDropEnabled, comparisonText, GoldPieces, loadCoins, Treasure } from "../coins";
import { useApp, useLoad } from "../state";
import { TrendChart } from "../charts";
import { formatDate, formatMoney } from "../format";
import { CategoryChip, Empty, GetStarted, Money, MonthNav } from "../ui";

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

function MoneyRemaining({ report }: { report: MonthReport }) {
  const { settings, droppedCoins, clearDroppedCoins } = useApp();
  const [coins] = useState(loadCoins);
  const remaining = report.projectedIncomeCents - report.projectedCents;
  const className = `stat-value${remaining < 0 ? " negative" : ""}`;
  const real = <Money cents={remaining} />;

  return (
    <div className="card">
      <div className="stat-label">Money remaining</div>
      <div className={className}>
        {coins.style === "gold" ? (
          <GoldPieces cents={remaining} real={formatMoney(remaining, settings.currency)} />
        ) : coins.style === "comparison" ? (
          comparisonText(remaining, coins)
        ) : (
          real
        )}
      </div>
      {coins.style === "comparison" && <div className="hint">{real}</div>}
      {report.upcomingCents > 0 && (
        <div className="hint">
          after <Money cents={report.upcomingCents} /> still due
        </div>
      )}
      {report.upcomingIncomeCents > 0 && (
        <div className="hint">
          includes <Money cents={report.upcomingIncomeCents} /> expected
        </div>
      )}
      {coins.style === "treasure" && (
        <Treasure
          remainingCents={remaining}
          incomeCents={report.projectedIncomeCents}
          dropped={coinDropEnabled(coins) ? droppedCoins : 0}
          onDropped={clearDroppedCoins}
        />
      )}
    </div>
  );
}

export default function Dashboard() {
  const { month, categories, setPage } = useApp();
  const report = useLoad(() => api.monthReport(month), [month]);
  if (!report) return null;

  const alerts = report.rows.filter((row) => row.budgetCents !== null && row.projectedCents > row.budgetCents);
  const isEmpty = report.trend.every((point) => point.incomeCents === 0 && point.expenseCents === 0);
  const scheduled = [...report.upcoming, ...report.upcomingIncome].sort((a, b) => a.date.localeCompare(b.date));

  return (
    <div className="page">
      <header className="page-header">
        <h1>{greeting()}</h1>
        <MonthNav />
      </header>

      {categories.length === 0 && <GetStarted onOwn={() => setPage("Categories")} />}

      <section className="hero-row">
        <div className="card">
          <div className="stat-label">Spent so far</div>
          <div className="hero-value">
            <Money cents={report.spentCents} />
          </div>
        </div>
        <MoneyRemaining report={report} />
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
          {report.upcomingIncomeCents > 0 && (
            <div className="hint">
              +<Money cents={report.upcomingIncomeCents} /> expected
            </div>
          )}
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
                  <CategoryChip id={row.categoryId} name={row.name} />
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
          {scheduled.length === 0 ? (
            <p className="hint">No scheduled payments left in this month.</p>
          ) : (
            scheduled.map((payment) => (
              <div className="status-line" key={`${payment.recurringItemId}-${payment.date}`}>
                <span>
                  {payment.name}
                  <br />
                  <span className="hint">{formatDate(payment.date)}</span>
                </span>
                <span className={`num${payment.kind === "income" ? " income-text" : ""}`}>
                  {payment.kind === "income" && "+"}
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
