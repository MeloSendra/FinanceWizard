import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, type Category, type Settings as AppSettings, type VaultStatus } from "./api";
import { AppContext } from "./state";
import { currentMonth, formatDate } from "./format";
import Budgets from "./pages/Budgets";
import Categories from "./pages/Categories";
import Dashboard from "./pages/Dashboard";
import Recurring from "./pages/Recurring";
import Settings from "./pages/Settings";
import Transactions from "./pages/Transactions";
import Unlock from "./pages/Unlock";

const PAGES = {
  Dashboard,
  Transactions,
  Categories,
  Recurring,
  Budgets,
  Settings,
};

type PageName = keyof typeof PAGES;

function Shell() {
  const [page, setPage] = useState<PageName>("Dashboard");
  const [categories, setCategories] = useState<Category[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [version, setVersion] = useState(0);
  const [month, setMonth] = useState(currentMonth);
  const [toast, setToast] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);

  const notify = useCallback((message: string) => {
    setToast(message);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setToast(null), 6000);
  }, []);
  const refresh = useCallback(() => setVersion((value) => value + 1), []);

  useEffect(() => {
    api
      .generateDuePayments()
      .then((created) => {
        if (created > 0) notify(`${created} scheduled ${created === 1 ? "payment" : "payments"} added.`);
        refresh();
      })
      .catch((error) => notify(String(error)));
  }, []);

  useEffect(() => {
    Promise.all([api.listCategories(), api.getSettings()])
      .then(([loadedCategories, loadedSettings]) => {
        setCategories(loadedCategories);
        setSettings(loadedSettings);
      })
      .catch((error) => notify(String(error)));
  }, [version]);

  const state = useMemo(
    () => settings && { categories, settings, version, refresh, notify, month, setMonth },
    [categories, settings, version, month],
  );
  if (!state) return null;

  const Page = PAGES[page];
  return (
    <AppContext.Provider value={state}>
      <div className="app">
        <aside className="sidebar">
          <div className="brand">FinanceWizard</div>
          <nav aria-label="Main">
            {(Object.keys(PAGES) as PageName[]).map((name) => (
              <button
                key={name}
                type="button"
                aria-current={name === page ? "page" : undefined}
                onClick={() => setPage(name)}
              >
                {name}
              </button>
            ))}
          </nav>
          <p className="backup-note">
            Last backup: {state.settings.lastBackupAt ? formatDate(state.settings.lastBackupAt) : "never"}
          </p>
        </aside>
        <main>
          <Page />
        </main>
      </div>
      {toast && (
        <div className="toast" role="status">
          {toast}
        </div>
      )}
    </AppContext.Provider>
  );
}

export default function App() {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    api
      .vaultStatus()
      .then(setStatus)
      .catch((error) => setFailure(String(error)));
  }, []);

  if (failure) return <p className="form-error">{failure}</p>;
  if (!status) return null;
  if (status === "unlocked") return <Shell />;
  return <Unlock status={status} onUnlocked={() => setStatus("unlocked")} />;
}
