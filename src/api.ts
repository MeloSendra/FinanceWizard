import { invoke } from "@tauri-apps/api/core";

export type Kind = "income" | "expense";
export type ItemType = "financing" | "subscription";
export type Frequency = "monthly" | "quarterly" | "yearly";
export type Status = "active" | "completed";

export interface Category {
  id: number;
  name: string;
  kind: Kind;
  parentId: number | null;
  defaultBudgetCents: number | null;
  archived: boolean;
  transactionCount: number;
  childCount: number;
  recurringCount: number;
}

export interface CategoryInput {
  name: string;
  kind: Kind;
  parentId: number | null;
  defaultBudgetCents: number | null;
}

export interface Transaction {
  id: number;
  date: string;
  amountCents: number;
  description: string;
  categoryId: number;
  categoryName: string;
  kind: Kind;
  recurringItemId: number | null;
  recurringItemName: string | null;
  scheduledDate: string | null;
}

export interface TransactionInput {
  date: string;
  amountCents: number;
  description: string;
  categoryId: number;
  recurringItemId: number | null;
}

export interface TransactionFilter {
  from?: string | null;
  to?: string | null;
  kind?: Kind | null;
  categoryId?: number | null;
}

export interface RecurringItem {
  id: number;
  name: string;
  itemType: ItemType;
  frequency: Frequency;
  startDate: string;
  endDate: string | null;
  status: Status;
  categoryId: number;
  categoryName: string;
  scheduleDay: number | null;
  generatedThrough: string;
  totalCents: number | null;
  installmentCount: number | null;
  openingPaidCents: number;
  openingPaidInstallments: number;
  paidCents: number;
  paidInstallments: number;
  currentAmountCents: number;
  nextPaymentDate: string | null;
}

export interface RecurringInput {
  name: string;
  itemType: ItemType;
  frequency: Frequency;
  startDate: string;
  endDate: string | null;
  status: Status;
  categoryId: number;
  scheduleDay: number | null;
  totalCents: number | null;
  installmentCount: number | null;
  openingPaidCents: number;
  openingPaidInstallments: number;
}

export interface ValueRecord {
  id: number;
  recurringItemId: number;
  effectiveDate: string;
  amountCents: number;
}

export interface UpcomingPayment {
  recurringItemId: number;
  name: string;
  date: string;
  amountCents: number;
  categoryId: number;
}

export interface BudgetRow {
  categoryId: number;
  name: string;
  depth: number;
  budgetCents: number | null;
  hasOverride: boolean;
  spentCents: number;
  upcomingCents: number;
  projectedCents: number;
}

export interface TrendPoint {
  month: string;
  incomeCents: number;
  expenseCents: number;
}

export interface MonthReport {
  month: string;
  incomeCents: number;
  spentCents: number;
  upcomingCents: number;
  projectedCents: number;
  budgetCents: number | null;
  rows: BudgetRow[];
  upcoming: UpcomingPayment[];
  trend: TrendPoint[];
}

export interface Settings {
  currency: string;
  lastBackupAt: string | null;
}

export type VaultStatus = "setup" | "locked" | "recovery" | "unlocked";

export interface BackupChoice {
  path: string;
  needsPassphrase: boolean;
}

export const api = {
  vaultStatus: () => invoke<VaultStatus>("vault_status"),
  createPin: (pin: string) => invoke<string>("create_pin", { pin }),
  unlock: (pin: string) => invoke<void>("unlock", { pin }),
  recover: (code: string, pin: string) => invoke<void>("recover", { code, pin }),
  changePin: (current: string, newPin: string) => invoke<void>("change_pin", { current, new: newPin }),

  listCategories: () => invoke<Category[]>("list_categories"),
  createCategory: (input: CategoryInput) => invoke<Category>("create_category", { input }),
  updateCategory: (id: number, input: CategoryInput) => invoke<Category>("update_category", { id, input }),
  setCategoryArchived: (id: number, archived: boolean) => invoke<Category>("set_category_archived", { id, archived }),
  deleteCategory: (id: number) => invoke<void>("delete_category", { id }),

  listTransactions: (filter: TransactionFilter) => invoke<Transaction[]>("list_transactions", { filter }),
  createTransaction: (input: TransactionInput) => invoke<Transaction>("create_transaction", { input }),
  updateTransaction: (id: number, input: TransactionInput) => invoke<Transaction>("update_transaction", { id, input }),
  deleteTransaction: (id: number) => invoke<void>("delete_transaction", { id }),

  generateDuePayments: () => invoke<number>("generate_due_payments"),
  listRecurring: () => invoke<RecurringItem[]>("list_recurring"),
  createRecurring: (input: RecurringInput, amountCents: number) =>
    invoke<RecurringItem>("create_recurring", { input, amountCents }),
  updateRecurring: (id: number, input: RecurringInput) => invoke<RecurringItem>("update_recurring", { id, input }),
  deleteRecurring: (id: number) => invoke<void>("delete_recurring", { id }),
  listRecurringValues: (itemId: number) => invoke<ValueRecord[]>("list_recurring_values", { itemId }),
  addRecurringValue: (itemId: number, effectiveDate: string, amountCents: number) =>
    invoke<void>("add_recurring_value", { itemId, effectiveDate, amountCents }),
  updateRecurringValue: (id: number, effectiveDate: string, amountCents: number) =>
    invoke<void>("update_recurring_value", { id, effectiveDate, amountCents }),
  deleteRecurringValue: (id: number) => invoke<void>("delete_recurring_value", { id }),

  monthReport: (month: string) => invoke<MonthReport>("month_report", { month }),
  setBudget: (month: string, categoryId: number, amountCents: number) =>
    invoke<void>("set_budget", { month, categoryId, amountCents }),
  clearBudget: (month: string, categoryId: number) => invoke<void>("clear_budget", { month, categoryId }),

  getSettings: () => invoke<Settings>("get_settings"),
  setCurrency: (currency: string) => invoke<void>("set_currency", { currency }),
  backupDatabase: (passphrase: string) => invoke<boolean>("backup_database", { passphrase }),
  chooseBackup: () => invoke<BackupChoice | null>("choose_backup"),
  restoreDatabase: (path: string, passphrase: string | null) =>
    invoke<void>("restore_database", { path, passphrase }),
  exportTransactions: () => invoke<boolean>("export_transactions"),
};
