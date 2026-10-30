import { useEffect, useRef, useState, type ReactNode } from "react";
import type { TrendPoint, ValueRecord } from "./api";
import { useApp } from "./state";
import { dayNumber, formatAxis, formatDate, formatMoney, formatMonth, formatMonthShort, todayIso } from "./format";

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
