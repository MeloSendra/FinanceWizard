import { createContext, useContext, useEffect, useState } from "react";
import type { Update } from "@tauri-apps/plugin-updater";
import type { Category, Settings } from "./api";

export interface AppState {
  categories: Category[];
  settings: Settings;
  /** Increases after every change, so views reload their derived data. */
  version: number;
  refresh: () => void;
  notify: (message: string) => void;
  month: string;
  setMonth: (month: string) => void;
  /** A newer version found by the update check, ready to install. */
  update: Update | null;
  setUpdate: (update: Update | null) => void;
  /** Payments created when the app opened, which Treasure shows falling. */
  droppedCoins: number;
  clearDroppedCoins: () => void;
}

export const AppContext = createContext<AppState | null>(null);

export function useApp(): AppState {
  const state = useContext(AppContext);
  if (!state) throw new Error("useApp needs an AppContext provider");
  return state;
}

export function useLoad<T>(load: () => Promise<T>, deps: unknown[]): T | null {
  const { version, notify } = useApp();
  const [data, setData] = useState<T | null>(null);
  useEffect(() => {
    let current = true;
    load()
      .then((value) => current && setData(value))
      .catch((error) => notify(String(error)));
    return () => {
      current = false;
    };
  }, [version, ...deps]);
  return data;
}
