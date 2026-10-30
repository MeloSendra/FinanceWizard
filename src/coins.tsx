import type { AnimationEvent } from "react";
import { formatFigure } from "./format";

export type CoinStyle = "plain" | "treasure" | "gold" | "comparison";

export interface CoinChoice {
  style: CoinStyle;
  /** What Comparison counts, in the plural. */
  item: string;
  priceCents: number;
  /** Follows the Windows animation setting until chosen here. */
  coinDrop?: boolean;
}

/** Kept with the theme, outside the database, as it is only decoration. */
const STORAGE_KEY = "financewizard-coins";
const DEFAULT_CHOICE: CoinChoice = { style: "plain", item: "coffees", priceCents: 250 };

export function loadCoins(): CoinChoice {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored ? { ...DEFAULT_CHOICE, ...JSON.parse(stored) } : DEFAULT_CHOICE;
  } catch {
    return DEFAULT_CHOICE;
  }
}

export function saveCoins(choice: CoinChoice) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(choice));
  } catch {
    // The choice still applies until the app closes.
  }
}

export function coinDropEnabled(choice: CoinChoice): boolean {
  return choice.coinDrop ?? !matchMedia("(prefers-reduced-motion: reduce)").matches;
}

interface Tier {
  title: string;
  coins: number;
  chest?: boolean;
}

/** A share of income, so the tiers mean the same at any income. */
export function treasureTier(remainingCents: number, incomeCents: number): Tier | null {
  if (remainingCents < 0) return null;
  const share = incomeCents > 0 ? remainingCents / incomeCents : 0;
  const within = (low: number, high: number, fewest: number, most: number) =>
    Math.round(fewest + ((Math.min(share, high) - low) / (high - low)) * (most - fewest));
  if (share < 0.1) return { title: "A few coppers", coins: within(0, 0.1, 1, 3) };
  if (share < 0.3) return { title: "Jingling pouch", coins: within(0.1, 0.3, 4, 9) };
  if (share < 0.6) return { title: "Heavy purse", coins: within(0.3, 0.6, 10, 16) };
  return { title: "Dragon's hoard", coins: within(0.6, 1, 17, 24), chest: true };
}

const COLUMNS = [60, 40, 80, 20, 100];
/** The columns in filling order, repeating. Favors the middle to form a heap. */
const FILL_ORDER = [0, 1, 2, 0, 3, 4, 1, 2];

function Coin({ x, y }: { x: number; y: number }) {
  return (
    <>
      <ellipse cx={x} cy={y + 2} rx={10} ry={4} fill="#b97f00" />
      <ellipse cx={x} cy={y} rx={10} ry={4} fill="#eda100" stroke="#c98500" />
    </>
  );
}

function Pile({ tier, dropped, onDropped }: { tier: Tier; dropped: number; onDropped: () => void }) {
  const floor = tier.chest ? 34 : 58;
  const heights = COLUMNS.map(() => 0);
  const coins = Array.from({ length: tier.coins }, (_, index) => {
    const column = FILL_ORDER[index % FILL_ORDER.length];
    return { x: COLUMNS[column], y: floor - heights[column]++ * 5 };
  }).sort((a, b) => b.y - a.y);
  const top = coins[coins.length - 1]?.y ?? floor;
  const falling = Math.min(dropped, 5);

  const finish = (event: AnimationEvent) => {
    if (event.currentTarget.getAttribute("data-last")) onDropped();
  };

  return (
    <svg className="coin-pile" viewBox="0 0 120 68" aria-hidden="true">
      {coins.map((coin, index) => (
        <Coin key={index} {...coin} />
      ))}
      {tier.chest && (
        <>
          <rect x={18} y={38} width={84} height={28} rx={3} fill="#8a5a2b" />
          <rect x={18} y={38} width={84} height={6} fill="#6f4722" />
          <rect x={54} y={38} width={12} height={15} rx={2} fill="#eda100" />
        </>
      )}
      {Array.from({ length: falling }, (_, index) => (
        <g
          key={index}
          className="coin-fall"
          data-last={index === falling - 1 ? true : undefined}
          style={{ animationDelay: `${index * 160}ms` }}
          onAnimationEnd={finish}
        >
          <Coin x={COLUMNS[index]} y={top - 6} />
        </g>
      ))}
    </svg>
  );
}

function EmptyPouch() {
  return (
    <svg className="coin-pile" viewBox="0 0 120 68" aria-hidden="true">
      <path d="M44 22 Q30 40 34 56 Q36 66 60 66 Q84 66 86 56 Q90 40 76 22 Z" fill="#8a5a2b" />
      <path d="M46 16 Q60 24 74 16 L76 24 Q60 30 44 24 Z" fill="#6f4722" />
      <rect x={72} y={40} width={36} height={20} rx={2} fill="#f0efec" stroke="#c3c2b7" transform="rotate(-6 90 50)" />
      <text x={90} y={54} textAnchor="middle" fontSize={12} fontWeight={600} fill="#0b0b0b" transform="rotate(-6 90 50)">
        IOU
      </text>
    </svg>
  );
}

export function Treasure(props: { remainingCents: number; incomeCents: number; dropped: number; onDropped: () => void }) {
  const tier = treasureTier(props.remainingCents, props.incomeCents);
  return (
    <div className="treasure">
      {tier ? <Pile tier={tier} dropped={props.dropped} onDropped={props.onDropped} /> : <EmptyPouch />}
      <div className="hint">{tier?.title ?? "In debt to the goblins"}</div>
    </div>
  );
}

function GoldIcon() {
  return (
    <svg className="gold-icon" viewBox="0 0 20 20" aria-hidden="true">
      <circle cx={10} cy={10} r={9} fill="#eda100" stroke="#b97f00" strokeWidth={2} />
      <circle cx={10} cy={10} r={5} fill="none" stroke="#b97f00" />
    </svg>
  );
}

export function GoldPieces({ cents, real }: { cents: number; real: string }) {
  return (
    <span className="gold-pieces" tabIndex={0} title={real} aria-label={real}>
      <GoldIcon />
      {formatFigure(Math.trunc(cents / 100) * 100)} gp
    </span>
  );
}

/** Whole numbers until the count is small, as in 541 coffees or 1.3 laptops. */
export function comparisonText(cents: number, { item, priceCents }: CoinChoice): string {
  const count = Math.abs(cents) / priceCents;
  const figure = count < 10 ? (Math.floor(count * 10) / 10).toString() : Math.floor(count).toLocaleString();
  const name = figure === "1" ? item.replace(/s$/, "") : item;
  return `${cents < 0 ? "Short " : ""}${figure} ${name}`;
}
