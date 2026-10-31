import type { Category, Kind } from "./api";

/** Reads what the user typed into cents without going through floats. */
export function parseAmount(text: string): number | null {
  const compact = text.trim().replace(/\s/g, "");
  if (!/^\d[\d.,]*$/.test(compact)) return null;
  // The last separator is the decimal one when one or two digits follow it.
  const decimal = compact.match(/[.,](\d{1,2})$/);
  const whole = (decimal ? compact.slice(0, decimal.index) : compact).replace(/[.,]/g, "");
  const cents = Number(whole) * 100 + Number((decimal?.[1] ?? "").padEnd(2, "0"));
  return Number.isSafeInteger(cents) ? cents : null;
}

export function amountText(cents: number | null | undefined): string {
  return cents == null ? "" : (cents / 100).toFixed(2);
}

export function formatMoney(cents: number, currency: string): string {
  return new Intl.NumberFormat(undefined, { style: "currency", currency }).format(cents / 100);
}

/** A plain figure with decimals only when it has them, as in 1,600 / 4,800. */
export function formatFigure(cents: number): string {
  return new Intl.NumberFormat(undefined, {
    minimumFractionDigits: cents % 100 === 0 ? 0 : 2,
    maximumFractionDigits: 2,
  }).format(cents / 100);
}

export function formatAxis(cents: number, compact: boolean): string {
  return new Intl.NumberFormat(undefined, {
    notation: compact ? "compact" : "standard",
    maximumFractionDigits: compact ? 1 : 2,
  }).format(cents / 100);
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

export function todayIso(): string {
  const now = new Date();
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

export function currentMonth(): string {
  return todayIso().slice(0, 7);
}

export function shiftMonth(month: string, by: number): string {
  const [year, index] = month.split("-").map(Number);
  const date = new Date(year, index - 1 + by, 1);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}`;
}

export function monthBounds(month: string): { from: string; to: string } {
  const [year, index] = month.split("-").map(Number);
  return { from: `${month}-01`, to: `${month}-${pad(new Date(year, index, 0).getDate())}` };
}

function parseLocal(iso: string): Date {
  const [year, month, day] = iso.slice(0, 10).split("-").map(Number);
  return new Date(year, month - 1, day);
}

export function formatDate(iso: string): string {
  return parseLocal(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

export function formatMonth(month: string): string {
  return parseLocal(`${month}-01`).toLocaleDateString(undefined, { month: "long", year: "numeric" });
}

export function formatMonthShort(month: string): string {
  return parseLocal(`${month}-01`).toLocaleDateString(undefined, { month: "short" });
}

export function dayNumber(iso: string): number {
  return parseLocal(iso).getTime() / 86_400_000;
}

function toIso(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function addDays(iso: string, days: number): string {
  const date = parseLocal(iso);
  date.setDate(date.getDate() + days);
  return toIso(date);
}

/** The Monday of the week containing `iso`. */
export function startOfWeek(iso: string): string {
  return addDays(iso, -((parseLocal(iso).getDay() + 6) % 7));
}

/** This week inside the current month, otherwise the month's last week. */
export function weekForMonth(month: string): string {
  const today = todayIso();
  return startOfWeek(today.startsWith(month) ? today : monthBounds(month).to);
}

export function formatWeekday(iso: string): string {
  return parseLocal(iso).toLocaleDateString(undefined, { weekday: "short" });
}

export function formatDayMonth(iso: string): string {
  return parseLocal(iso).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** Used for categories without a chosen color, and offered first for new ones.
 * This order keeps neighbouring hues apart for color-blind readers. */
const CATEGORY_PALETTE = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"];

export function paletteColor(id: number): string {
  return CATEGORY_PALETTE[id % CATEGORY_PALETTE.length];
}

export function nextPaletteColor(categories: Category[]): string {
  const used = new Set(categories.filter((category) => category.parentId === null).map((c) => c.color ?? paletteColor(c.id)));
  return CATEGORY_PALETTE.find((color) => !used.has(color)) ?? paletteColor(categories.length);
}

/** Top-level categories show their own color. Subcategories show a darker shade
 * of it, which differs between siblings so charts can tell them apart. */
export function categoryColor(category: Category, categories: Category[]): string {
  const byId = new Map(categories.map((entry) => [entry.id, entry]));
  let root = category;
  while (root.parentId !== null && byId.has(root.parentId)) root = byId.get(root.parentId) as Category;
  const base = root.color ?? paletteColor(root.id);
  if (root.id === category.id) return base;
  const siblings = categories.filter((entry) => entry.parentId === category.parentId).map((entry) => entry.id);
  const shade = Math.min(16 + siblings.indexOf(category.id) * 14, 58);
  return `color-mix(in srgb, ${base} ${100 - shade}%, black)`;
}

export interface TreeEntry {
  category: Category;
  depth: number;
}

/** Categories in tree order, each after its parent. */
export function categoryTree(categories: Category[], kind?: Kind): TreeEntry[] {
  const children = new Map<number | null, Category[]>();
  for (const category of categories) {
    if (kind && category.kind !== kind) continue;
    children.set(category.parentId, [...(children.get(category.parentId) ?? []), category]);
  }
  const entries: TreeEntry[] = [];
  const visit = (parentId: number | null, depth: number) => {
    for (const category of children.get(parentId) ?? []) {
      entries.push({ category, depth });
      visit(category.id, depth + 1);
    }
  };
  visit(null, 0);
  return entries;
}

/** Leaves out subcategories of parents that aren't expanded. */
export function expandedTree(entries: TreeEntry[], expanded: number[]): TreeEntry[] {
  const listed = new Set(entries.map(({ category }) => category.id));
  return entries.filter(({ category: { parentId } }) => parentId === null || !listed.has(parentId) || expanded.includes(parentId));
}

/** Entries whose name, or whose parent's name, contains `search`. */
export function searchTree(entries: TreeEntry[], search: string): TreeEntry[] {
  const needle = search.trim().toLowerCase();
  const names = new Map(entries.map(({ category }) => [category.id, category.name.toLowerCase()]));
  return entries.filter(
    ({ category }) => names.get(category.id)?.includes(needle) || (category.parentId !== null && names.get(category.parentId)?.includes(needle)),
  );
}
