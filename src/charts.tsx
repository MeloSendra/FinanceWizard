import { useEffect, useRef, useState, type ReactNode } from "react";
import type { BudgetRow, TrendPoint, ValueRecord } from "./api";
import { useApp } from "./state";
import {
  categoryColor,
  dayNumber,
  formatAxis,
  formatDate,
  formatDayMonth,
  formatFigure,
  formatMoney,
  formatMonth,
  formatMonthShort,
  formatWeekday,
  todayIso,
} from "./format";

const MARGIN = { top: 10, right: 12, bottom: 24, left: 52 };

function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    if (!ref.current) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

function ticks(low: number, high: number, count = 4): number[] {
  const rough = Math.max(high - low, 1) / count;
  const magnitude = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * magnitude).find((candidate) => candidate >= rough) ?? rough;
  const values = [];
  for (let value = Math.floor(low / step) * step; value < high + step; value += step) values.push(value);
  return values;
}

function YAxis(props: { values: number[]; y: (value: number) => number; width: number }) {
  const compact = Math.max(...props.values.map(Math.abs)) >= 1_000_000;
  return (
    <>
      {props.values.map((value) => (
        <g key={value}>
          <line x1={MARGIN.left} x2={props.width - MARGIN.right} y1={props.y(value)} y2={props.y(value)} stroke="var(--grid)" />
          <text x={MARGIN.left - 8} y={props.y(value) + 4} textAnchor="end">
            {formatAxis(value, compact)}
          </text>
        </g>
      ))}
    </>
  );
}

function Tip({ x, width, children }: { x: number; width: number; children: ReactNode }) {
  const left = x > width / 2;
  return (
    <div className="chart-tip" style={left ? { right: width - x + 8, top: 8 } : { left: x + 8, top: 8 }}>
      {children}
    </div>
  );
}

function topRounded(x: number, top: number, width: number, bottom: number): string {
  const radius = Math.min(4, bottom - top, width / 2);
  return `M${x},${bottom}V${top + radius}Q${x},${top} ${x + radius},${top}H${x + width - radius}Q${x + width},${top} ${x + width},${top + radius}V${bottom}Z`;
}

const SERIES = [
  { key: "incomeCents", label: "Income", color: "var(--income)" },
  { key: "expenseCents", label: "Expenses", color: "var(--expense)" },
] as const;

export function TrendChart({ points }: { points: TrendPoint[] }) {
  const { settings } = useApp();
  const { ref, width } = useWidth();
  const [active, setActive] = useState<number | null>(null);
  const [showTable, setShowTable] = useState(false);
  const money = (cents: number) => formatMoney(cents, settings.currency);

  const height = 240;
  const max = Math.max(...points.flatMap((point) => [point.incomeCents, point.expenseCents]), 0);
  const values = ticks(0, max);
  const top = values[values.length - 1];
  const y = (value: number) => MARGIN.top + (1 - value / top) * (height - MARGIN.top - MARGIN.bottom);
  const band = (width - MARGIN.left - MARGIN.right) / points.length;
  const barWidth = Math.min(24, Math.max(2, (band - 6) / 2));
  const labelEvery = band >= 34 ? 1 : band >= 18 ? 2 : 3;
  const baseline = y(0);

  return (
    <>
      <div className="chart-head">
        <div className="legend">
          {SERIES.map((series) => (
            <span key={series.key}>
              <i className="key" style={{ background: series.color }} />
              {series.label}
            </span>
          ))}
        </div>
        <button type="button" className="ghost small" onClick={() => setShowTable(!showTable)}>
          {showTable ? "Show chart" : "Show table"}
        </button>
      </div>
      {showTable ? (
        <table>
          <thead>
            <tr>
              <th>Month</th>
              <th className="num">Income</th>
              <th className="num">Expenses</th>
            </tr>
          </thead>
          <tbody>
            {points.map((point) => (
              <tr key={point.month}>
                <td>{formatMonth(point.month)}</td>
                <td className="num">{money(point.incomeCents)}</td>
                <td className="num">{money(point.expenseCents)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div className="chart" ref={ref} onMouseLeave={() => setActive(null)}>
          {width > 0 && (
            <svg height={height} role="img" aria-label="Income and expenses for the last twelve months">
              <YAxis values={values} y={y} width={width} />
              <line x1={MARGIN.left} x2={width - MARGIN.right} y1={baseline} y2={baseline} stroke="var(--axis)" />
              {points.map((point, index) => {
                const x = MARGIN.left + index * band;
                const middle = x + band / 2;
                const showLabel = (points.length - 1 - index) % labelEvery === 0;
                return (
                  <g key={point.month}>
                    <rect
                      className="band"
                      x={x}
                      y={MARGIN.top}
                      width={band}
                      height={baseline - MARGIN.top}
                      tabIndex={0}
                      aria-label={`${formatMonth(point.month)}: income ${money(point.incomeCents)}, expenses ${money(point.expenseCents)}`}
                      onMouseEnter={() => setActive(index)}
                      onFocus={() => setActive(index)}
                      onBlur={() => setActive(null)}
                    />
                    {SERIES.map((series, position) => {
                      const cents = point[series.key];
                      return cents > 0 ? (
                        <path
                          key={series.key}
                          d={topRounded(middle + (position === 0 ? -barWidth - 1 : 1), y(cents), barWidth, baseline)}
                          fill={series.color}
                          pointerEvents="none"
                        />
                      ) : null;
                    })}
                    {showLabel && (
                      <text x={middle} y={height - 7} textAnchor="middle">
                        {formatMonthShort(point.month)}
                      </text>
                    )}
                  </g>
                );
              })}
            </svg>
          )}
          {active !== null && (
            <Tip x={MARGIN.left + (active + 0.5) * band} width={width}>
              <strong>{formatMonth(points[active].month)}</strong>
              {SERIES.map((series) => (
                <div key={series.key}>
                  <i className="key" style={{ background: series.color }} />
                  {series.label}: {money(points[active][series.key])}
                </div>
              ))}
            </Tip>
          )}
        </div>
      )}
    </>
  );
}

/** Each value holds until the next one takes effect, so this draws steps. */
export function ValueChart({ values }: { values: ValueRecord[] }) {
  const { settings } = useApp();
  const { ref, width } = useWidth();
  const [active, setActive] = useState<number | null>(null);

  const height = 150;
  const amounts = values.map((value) => value.amountCents);
  const low = Math.min(...amounts);
  const high = Math.max(...amounts);
  const padding = Math.max((high - low) * 0.25, high * 0.05, 100);
  const axis = ticks(Math.max(low - padding, 0), high + padding, 3);
  const bottom = axis[0];
  const top = axis[axis.length - 1];
  const first = dayNumber(values[0].effectiveDate);
  const last = Math.max(dayNumber(todayIso()), dayNumber(values[values.length - 1].effectiveDate), first + 1);
  const x = (iso: string) =>
    MARGIN.left + ((dayNumber(iso) - first) / (last - first)) * (width - MARGIN.left - MARGIN.right - 6);
  const y = (cents: number) =>
    MARGIN.top + (1 - (cents - bottom) / (top - bottom)) * (height - MARGIN.top - MARGIN.bottom);

  let line = `M${x(values[0].effectiveDate)},${y(values[0].amountCents)}`;
  for (const value of values.slice(1)) line += `H${x(value.effectiveDate)}V${y(value.amountCents)}`;
  line += `H${width - MARGIN.right - 6}`;

  return (
    <div className="chart" ref={ref} onMouseLeave={() => setActive(null)}>
      {width > 0 && (
        <svg height={height} role="img" aria-label="Value history">
          <YAxis values={axis} y={y} width={width} />
          <path d={line} fill="none" stroke="var(--accent)" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
          {values.map((value, index) => (
            <g key={value.id}>
              <circle cx={x(value.effectiveDate)} cy={y(value.amountCents)} r={4} fill="var(--accent)" stroke="var(--surface)" strokeWidth={2} />
              <circle
                className="band"
                cx={x(value.effectiveDate)}
                cy={y(value.amountCents)}
                r={12}
                tabIndex={0}
                aria-label={`${formatDate(value.effectiveDate)}: ${formatMoney(value.amountCents, settings.currency)}`}
                onMouseEnter={() => setActive(index)}
                onFocus={() => setActive(index)}
                onBlur={() => setActive(null)}
              />
            </g>
          ))}
        </svg>
      )}
      {active !== null && (
        <Tip x={x(values[active].effectiveDate)} width={width}>
          <strong>{formatMoney(values[active].amountCents, settings.currency)}</strong>
          from {formatDate(values[active].effectiveDate)}
        </Tip>
      )}
    </div>
  );
}

export interface DayTotal {
  date: string;
  cents: number;
}

export function WeekChart({ days }: { days: DayTotal[] }) {
  const { settings } = useApp();
  const { ref, width } = useWidth();
  const [active, setActive] = useState<number | null>(null);
  const [showTable, setShowTable] = useState(false);
  const money = (cents: number) => formatMoney(cents, settings.currency);

  const height = 220;
  const top = 22;
  const values = ticks(0, Math.max(...days.map((day) => day.cents), 0), 3);
  const highest = values[values.length - 1];
  const y = (value: number) => top + (1 - value / highest) * (height - top - MARGIN.bottom);
  const band = (width - MARGIN.left - MARGIN.right) / days.length;
  const barWidth = Math.min(40, Math.max(6, band - 12));
  const baseline = y(0);
  const today = todayIso();
  const label = (cents: number) => (band < 58 ? formatAxis(cents, true) : formatFigure(cents));

  return (
    <>
      <div className="chart-head">
        <span className="hint">Expenses per day</span>
        <button type="button" className="ghost small" onClick={() => setShowTable(!showTable)}>
          {showTable ? "Show chart" : "Show table"}
        </button>
      </div>
      {showTable ? (
        <table>
          <thead>
            <tr>
              <th>Day</th>
              <th className="num">Spent</th>
            </tr>
          </thead>
          <tbody>
            {days.map((day) => (
              <tr key={day.date}>
                <td>{formatDate(day.date)}</td>
                <td className="num">{money(day.cents)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div className="chart" ref={ref} onMouseLeave={() => setActive(null)}>
          {width > 0 && (
            <svg height={height} role="img" aria-label="Expenses for each day of the week">
              <YAxis values={values} y={y} width={width} />
              <line x1={MARGIN.left} x2={width - MARGIN.right} y1={baseline} y2={baseline} stroke="var(--axis)" />
              {days.map((day, index) => {
                const x = MARGIN.left + index * band;
                const middle = x + band / 2;
                return (
                  <g key={day.date}>
                    <rect
                      className="band"
                      x={x}
                      y={0}
                      width={band}
                      height={baseline}
                      tabIndex={0}
                      aria-label={`${formatDate(day.date)}: ${money(day.cents)}`}
                      onMouseEnter={() => setActive(index)}
                      onFocus={() => setActive(index)}
                      onBlur={() => setActive(null)}
                    />
                    {day.cents > 0 && (
                      <>
                        <path d={topRounded(middle - barWidth / 2, y(day.cents), barWidth, baseline)} fill="var(--expense)" pointerEvents="none" />
                        <text className="bar-value" x={middle} y={y(day.cents) - 6} textAnchor="middle" pointerEvents="none">
                          {label(day.cents)}
                        </text>
                      </>
                    )}
                    <text className={day.date === today ? "today" : undefined} x={middle} y={height - 7} textAnchor="middle">
                      {formatWeekday(day.date)}
                    </text>
                  </g>
                );
              })}
            </svg>
          )}
          {active !== null && (
            <Tip x={MARGIN.left + (active + 0.5) * band} width={width}>
              <strong>{formatDayMonth(days[active].date)}</strong>
              {money(days[active].cents)}
            </Tip>
          )}
        </div>
      )}
    </>
  );
}

interface Slice {
  key: string;
  name: string;
  cents: number;
  color: string;
  /** The category this slice opens, when it has subcategories with spending. */
  opens?: number;
}

/** Beyond this, the smallest slices fold into Other. */
const MAX_SLICES = 7;

function foldSmallest(slices: Slice[]): Slice[] {
  const ranked = [...slices].sort((a, b) => b.cents - a.cents);
  if (ranked.length <= MAX_SLICES) return ranked;
  const rest = ranked.slice(MAX_SLICES - 1);
  const cents = rest.reduce((sum, slice) => sum + slice.cents, 0);
  return [...ranked.slice(0, MAX_SLICES - 1), { key: "other", name: `Other (${rest.length})`, cents, color: "var(--axis)" }];
}

function ringSlice(start: number, end: number, outer: number, inner: number): string {
  const point = (radius: number, turn: number) => {
    const angle = 2 * Math.PI * turn - Math.PI / 2;
    return `${100 + radius * Math.cos(angle)},${100 + radius * Math.sin(angle)}`;
  };
  // A full circle has identical end points, which would draw nothing.
  const stop = Math.min(end, start + 0.9999);
  const large = stop - start > 0.5 ? 1 : 0;
  return `M${point(outer, start)}A${outer},${outer} 0 ${large} 1 ${point(outer, stop)}L${point(inner, stop)}A${inner},${inner} 0 ${large} 0 ${point(inner, start)}Z`;
}

export function CategoryDonut({ rows }: { rows: BudgetRow[] }) {
  const { categories, settings } = useApp();
  const [openedId, setOpenedId] = useState<number | null>(null);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const money = (cents: number) => formatMoney(cents, settings.currency);

  const spending = rows.filter((row) => row.spentCents > 0);
  const colorOf = (id: number) => {
    const category = categories.find((entry) => entry.id === id);
    return category ? categoryColor(category, categories) : "var(--axis)";
  };
  const childrenOf = (id: number) =>
    spending.filter((row) => categories.find((entry) => entry.id === row.categoryId)?.parentId === id);

  const toSlice = (row: BudgetRow): Slice => ({
    key: String(row.categoryId),
    name: row.name,
    cents: row.spentCents,
    color: colorOf(row.categoryId),
    opens: childrenOf(row.categoryId).length > 0 ? row.categoryId : undefined,
  });

  const opened = spending.find((row) => row.categoryId === openedId && childrenOf(row.categoryId).length > 0);
  let slices: Slice[];
  if (opened) {
    const children = childrenOf(opened.categoryId);
    const direct = opened.spentCents - children.reduce((sum, row) => sum + row.spentCents, 0);
    slices = children.map((row) => ({ ...toSlice(row), opens: undefined }));
    if (direct > 0) slices.push({ key: "direct", name: `${opened.name} (directly)`, cents: direct, color: colorOf(opened.categoryId) });
  } else {
    slices = spending.filter((row) => row.depth === 0).map(toSlice);
  }
  slices = foldSmallest(slices);

  const total = slices.reduce((sum, slice) => sum + slice.cents, 0);
  const active = slices.find((slice) => slice.key === activeKey);
  let turn = 0;

  return (
    <div className="donut">
      <svg viewBox="0 0 200 200" role="img" aria-label={`Spending by category, ${money(total)} in total`} onMouseLeave={() => setActiveKey(null)}>
        {slices.map((slice) => {
          const start = turn;
          turn += slice.cents / total;
          return (
            <path
              key={slice.key}
              className={slice.opens !== undefined ? "opens" : undefined}
              d={ringSlice(start, turn, 92, 62)}
              style={{ fill: slice.color, opacity: active && active !== slice ? 0.45 : 1 }}
              stroke="var(--surface)"
              strokeWidth={2}
              onMouseEnter={() => setActiveKey(slice.key)}
              onClick={() => slice.opens !== undefined && setOpenedId(slice.opens)}
            />
          );
        })}
        <text x={100} y={94} textAnchor="middle" className="donut-label">
          {active ? `${Math.round((active.cents / total) * 100)}%` : opened ? "Subtotal" : "Spent"}
        </text>
        <text x={100} y={114} textAnchor="middle" className="donut-total">
          {formatFigure(active?.cents ?? total)}
        </text>
      </svg>
      <div className="donut-side">
        {opened && (
          <button type="button" className="ghost small" onClick={() => setOpenedId(null)}>
            ‹ All categories
          </button>
        )}
        <h3 className="field-label">{opened ? opened.name : "By category"}</h3>
        <ul className="donut-legend">
          {slices.map((slice) => {
            const content = (
              <>
                <i className="key" style={{ background: slice.color }} />
                <span className="donut-name">{slice.name}</span>
                <span className="num">{money(slice.cents)}</span>
                <span className="num hint">{Math.round((slice.cents / total) * 100)}%</span>
              </>
            );
            return (
              <li key={slice.key} onMouseEnter={() => setActiveKey(slice.key)} onMouseLeave={() => setActiveKey(null)}>
                {slice.opens !== undefined ? (
                  <button
                    type="button"
                    className="donut-row"
                    onClick={() => setOpenedId(slice.opens as number)}
                    onFocus={() => setActiveKey(slice.key)}
                    onBlur={() => setActiveKey(null)}
                    aria-label={`${slice.name}, ${money(slice.cents)}. Show subcategories`}
                  >
                    {content}
                  </button>
                ) : (
                  <div className="donut-row">{content}</div>
                )}
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
