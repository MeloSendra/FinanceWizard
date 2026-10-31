# FinanceWizard architecture and MVP plan

## Problem and proposed approach

FinanceWizard should be a Windows-first, single-user local desktop app for manually recording income, expenses, recurring financial commitments, historical value changes, and budgets. The application should persist data safely between sessions and be distributable as a standalone executable.

The proposed foundation is:

- A web-based UI in a desktop shell, preferably Tauri, so the UI can later support polished scrolling and transition animations without changing the product architecture.
- A local SQLite database as the durable source of truth.
- A typed application boundary between the UI and database operations; the UI should not write arbitrary SQL.
- One configured currency for the MVP. Multi-currency conversion is explicitly deferred.

This is a sound fit for the requested product. SQLite is appropriate for a single-user local finance manager, while Tauri avoids the heavier memory footprint of an Electron application and still gives the UI the animation capabilities of the web platform.

The owner has confirmed this direction: Windows-only, local desktop, web UI in a desktop shell, manual-entry MVP, and one configured currency.

## MVP scope

Included:

- Manual creation, editing, viewing, and deletion of categories.
- Two-level parent/child category hierarchy with validation against cycles and invalid deletion.
- Manual income and expense transactions.
- Monthly transaction views with filtering by date, type, and category.
- Recurring items for financing and subscriptions.
- Optional scheduled payments on recurring items, which create transactions automatically on their assigned date.
- Historical value records for recurring items, such as rent changing from 500 to 510.
- Income and expense dashboard summaries and graphs.
- Optional monthly budget values attached to categories, with actual-versus-budget comparisons.
- Local persistence, clear destructive-action confirmation, and backup, restore, and export.

Deferred:

- Bank synchronization and automatic transaction import.
- Multi-user accounts, cloud sync, authentication, and remote APIs.
- Multi-currency conversion.
- Automatic financial advice or forecasting.
- Advanced animation work until the information architecture is stable.

## Core model

Use separate concepts rather than treating every record as a transaction:

- `categories`: self-referencing parent/child hierarchy, declared kind (`income` or `expense`), optional default monthly budget, and active/archive state.
- `transactions`: dated income or expense events with amount, description, category, optional link to a recurring item, and the scheduled date when generated from a schedule.
- `recurring_items`: financing or subscription metadata, frequency, start/end dates, active/completed state, type, optional schedule (assigned day and category for generated transactions), and the date through which payments have been generated. Financing items also store the total amount payable including interest, the number of installments, and an opening balance of amount and installments already paid outside the app.
- `recurring_value_history`: effective date plus amount for each recurring item; this is the source for historical price graphs.
- `budgets`: explicit month/category overrides or targets where the category-level default is insufficient.
- Budget comparison queries: actual spending should be calculated from transactions rather than duplicated as editable totals.

Amounts should be stored as integer minor units (for example cents), never floating-point values. Dates should be stored in an unambiguous ISO representation. Historical values need an effective date and a rule for which value applies in a given month.

## UI structure

Use a small, stable navigation structure:

1. Dashboard: greeting, selected-month income/expense summary with spent-so-far and projected end-of-month totals, trend graph, and budget alerts.
2. Transactions: current-month list, month navigation, filters, and create/edit detail view.
3. Categories: tree view, category detail, optional monthly budget, and usage/history view.
4. Recurring: financing and subscriptions in one area, with type-specific fields and progress indicators. Financing shows installment progress as one continuous bar regardless of term length, with unlabelled marks at every 25%: a line in the unfilled track and/or an indent on the filled bar. Hovering the bar shows the numbers (8 of 24), and they must also be reachable by keyboard focus. The amount sits beside it as plain text (1,600 / 4,800).
5. Budgets: planned versus spent-so-far and projected by month and category, reusing category budgets where no explicit budget exists.
6. Savings: total saved, the running month, and history by month.
7. Settings: backup, restore, and export actions.

The first UI pass should prioritize hierarchy, readable tables/cards, keyboard-accessible forms, empty states, validation messages, and confirmation for destructive actions. Graphs should be derived from query results rather than stored separately.

## Important behavior decisions

- Deleting a category that has transactions or descendants should be blocked or replaced with archive/deactivate behavior; hard deletion would damage historical reporting.
- Deleting a transaction or historical value should require confirmation and update all derived summaries immediately.
- A recurring item's value history should be appendable and editable, but overlapping effective periods should be rejected.
- Financing progress is computed from recorded payments, meaning transactions linked to the recurring item plus the opening balance, not from the schedule or manually entered percentages.
- The total payable is entered by the user, so interest is included and it does not have to equal the installment amount times the number of installments.
- A down payment or anything paid before the item was added goes in the opening balance. It counts toward the paid amount but creates no transaction, so it cannot distort the spending or budget of the month it was entered. Only the installments part of the opening balance counts toward installment progress.
- Scheduled payments are optional and available for both financing and subscriptions. A recurring item without a schedule is tracked only through manually recorded payments.
- Scheduled payments post automatically with no approval step; mistakes are corrected by editing or deleting the transaction.
- The app only runs while open, so generation happens on launch: it creates a transaction for every assigned date that has passed since the item's generated-through date, including dates missed while the app was closed.
- Generation stops at the item's end date, or for financing once the installment count is reached.
- A generated transaction uses the value from `recurring_value_history` that applies on its scheduled date, and is an ordinary transaction afterwards that can be edited or deleted.
- Each recurring item can have only one transaction per scheduled date, and the generated-through date is advanced as payments are created, so a deleted generated transaction does not reappear on the next launch.
- Category budgets are defaults; explicit budget records should override the default for a specific month.
- Every category declares a kind, income or expense. A subcategory inherits its parent's kind, and a transaction can only use a category of its own type.
- “Monthly expense” means transactions whose date falls in the selected calendar month; recurring projections should be displayed separately from actuals.
- Monthly views show two figures: spent so far, from transactions, and projected end of month, which is spent so far plus scheduled payments still due that month. Both are calculated per category as well as in total, and neither is stored.
- Upcoming payments use the value that applies on their scheduled date and the category set on their schedule. Past months show actuals only, and future months show only the upcoming payments.
- A recurring item's kind comes from its category, so recurring income such as a salary is a non-financing item with an income category. It posts on its scheduled date like any other item. Expected income is kept apart from spending and budgets, and money remaining is projected income minus projected spending.
- A completed item reopens when an edit, or a deleted or unlinked payment, undoes what finished it. Items the user completed by hand with payments left stay completed. Dates passed in between appear under Past payments rather than posting automatically.
- Savings are calculated, never stored: an opening balance plus each finished month's income minus its expenses, from a chosen start month or the first transaction. Overspent months subtract, the current month does not count until it ends, and savings are never part of money remaining.
- Budget alerts use the projected figure, so a category is flagged as soon as its scheduled payments will take it over budget.
- Recurring items without a schedule have no due date, so they are not part of the projection.
- Backups use SQLite's own backup mechanism rather than copying the live file, and produce a single restorable file.
- Restoring a backup replaces all current data, so it requires explicit confirmation and takes a safety backup of the current database first.
- Export writes transactions to CSV for use outside the app. Export is one-way; CSV import is not part of the MVP.

## Delivery sequence

1. Scaffold the Tauri/web application and local SQLite persistence.
2. Implement schema, migrations, typed data-access commands, and validation.
3. Build categories and transactions first, including tests for money/date/category rules.
4. Add recurring items, historical values, and scheduled payment generation.
5. Add dashboard and budget queries/visualizations.
6. Add backup, restore, and CSV export.
7. Package a Windows installer/executable and document backup/export behavior.

## Risks and constraints

- A local-only app can lose data if the database file is corrupted or the user does not back it up; the MVP backup action reduces this risk only if it is used, so the app should make the last backup date visible.
- SQLite is not the right foundation if cloud sync, multiple simultaneous users, or bank-provider integrations become requirements; those should trigger a deliberate migration to a server database rather than being hidden behind premature abstractions.
- Tauri introduces a Rust toolchain and packaging complexity. If the project owner strongly prefers a single-language stack, a native .NET desktop app is a valid Windows-only alternative, but it gives up some web UI reuse and animation flexibility.
- Payment generation runs at launch, so transactions for dates passed while the app was closed appear only the next time it is opened.
