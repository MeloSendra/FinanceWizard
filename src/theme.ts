export type ThemeMode = "system" | "dark" | "dim" | "light" | "custom";

export interface ThemeChoice {
  mode: ThemeMode;
  background: string;
  accent: string;
}

/** Kept in the browser, not the database, so the lock screen can use it. */
const STORAGE_KEY = "financewizard-theme";
const DEFAULT_THEME: ThemeChoice = { mode: "system", background: "#20242c", accent: "#3987e5" };
const CUSTOM_TOKENS = [
  "page", "surface", "ink", "ink-2", "muted", "grid", "axis", "border", "wash",
  "accent", "accent-ink", "on-accent", "track", "income", "good-ink", "critical-ink",
];

type Rgb = [number, number, number];

function toRgb(hex: string): Rgb {
  return [1, 3, 5].map((start) => parseInt(hex.slice(start, start + 2), 16)) as Rgb;
}

function toHex(rgb: Rgb): string {
  return `#${rgb.map((value) => Math.round(value).toString(16).padStart(2, "0")).join("")}`;
}

function mix(from: string, to: string, share: number): string {
  const [a, b] = [toRgb(from), toRgb(to)];
  return toHex(a.map((value, index) => value + (b[index] - value) * share) as Rgb);
}

function luminance(hex: string): number {
  const [r, g, b] = toRgb(hex).map((value) => {
    const channel = value / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (light + 0.05) / (dark + 0.05);
}

/** Moves `color` toward `ink` until it can be read on `surface`. On mid-gray
 * surfaces a color can't reach full contrast and keep its hue, so red and green
 * accept less: their amounts also carry a sign. */
function readable(color: string, surface: string, ink: string, target = 4.5): string {
  let result = color;
  for (let share = 0.1; contrast(result, surface) < target && share <= 1; share += 0.1) {
    result = mix(color, ink, share);
  }
  return result;
}

/** Derives every color from two choices, picking text for contrast so that no
 * pair of choices can make the app unreadable. */
export function customTokens(background: string, accent: string): Record<string, string> {
  const ink = contrast("#f0efec", background) >= contrast("#0b0b0b", background) ? "#f0efec" : "#0b0b0b";
  const isDark = ink === "#f0efec";
  const surface = mix(background, ink, 0.05);
  return {
    page: background,
    surface,
    ink,
    "ink-2": mix(ink, background, 0.2),
    muted: mix(ink, background, 0.32),
    grid: mix(background, ink, 0.13),
    axis: mix(background, ink, 0.24),
    border: mix(background, ink, 0.16),
    wash: mix(background, ink, 0.09),
    accent,
    "accent-ink": readable(accent, surface, ink),
    "on-accent": contrast("#ffffff", accent) >= contrast("#0b0b0b", accent) ? "#ffffff" : "#0b0b0b",
    track: mix(surface, accent, 0.3),
    income: accent,
    "good-ink": readable(isDark ? "#0ca30c" : "#006300", surface, ink, 3),
    "critical-ink": readable(isDark ? "#e66767" : "#b02a2a", surface, ink, 3),
  };
}

export function loadTheme(): ThemeChoice {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored ? { ...DEFAULT_THEME, ...JSON.parse(stored) } : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

export function applyTheme(choice: ThemeChoice) {
  const root = document.documentElement;
  for (const token of CUSTOM_TOKENS) root.style.removeProperty(`--${token}`);
  root.style.removeProperty("color-scheme");
  if (choice.mode === "system") {
    delete root.dataset.theme;
    return;
  }
  root.dataset.theme = choice.mode;
  if (choice.mode === "custom") {
    const tokens = customTokens(choice.background, choice.accent);
    for (const [token, value] of Object.entries(tokens)) root.style.setProperty(`--${token}`, value);
    root.style.setProperty("color-scheme", tokens.ink === "#f0efec" ? "dark" : "light");
  }
}

export function saveTheme(choice: ThemeChoice) {
  applyTheme(choice);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(choice));
  } catch {
    // The choice still applies until the app closes.
  }
}
