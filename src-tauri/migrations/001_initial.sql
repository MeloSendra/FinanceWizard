CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE categories (
    id                   INTEGER PRIMARY KEY,
    name                 TEXT NOT NULL CHECK (length(trim(name)) > 0),
    kind                 TEXT NOT NULL CHECK (kind IN ('income', 'expense')),
    parent_id            INTEGER REFERENCES categories (id) ON DELETE RESTRICT,
    default_budget_cents INTEGER CHECK (default_budget_cents >= 0),
    archived             INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1))
);

CREATE UNIQUE INDEX categories_unique_sibling_name
    ON categories (COALESCE(parent_id, 0), name COLLATE NOCASE);

CREATE TABLE recurring_items (
    id                        INTEGER PRIMARY KEY,
    name                      TEXT NOT NULL CHECK (length(trim(name)) > 0),
    item_type                 TEXT NOT NULL CHECK (item_type IN ('financing', 'subscription')),
    frequency                 TEXT NOT NULL CHECK (frequency IN ('monthly', 'quarterly', 'yearly')),
    start_date                TEXT NOT NULL,
    end_date                  TEXT,
    status                    TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed')),
    category_id               INTEGER NOT NULL REFERENCES categories (id) ON DELETE RESTRICT,
    schedule_day              INTEGER CHECK (schedule_day BETWEEN 1 AND 31),
    generated_through         TEXT NOT NULL,
    total_cents               INTEGER CHECK (total_cents > 0),
    installment_count         INTEGER CHECK (installment_count > 0),
    opening_paid_cents        INTEGER NOT NULL DEFAULT 0 CHECK (opening_paid_cents >= 0),
    opening_paid_installments INTEGER NOT NULL DEFAULT 0 CHECK (opening_paid_installments >= 0)
);

CREATE TABLE recurring_value_history (
    id                INTEGER PRIMARY KEY,
    recurring_item_id INTEGER NOT NULL REFERENCES recurring_items (id) ON DELETE CASCADE,
    effective_date    TEXT NOT NULL,
    amount_cents      INTEGER NOT NULL CHECK (amount_cents > 0),
    UNIQUE (recurring_item_id, effective_date)
);

CREATE TABLE transactions (
    id                INTEGER PRIMARY KEY,
    date              TEXT NOT NULL,
    amount_cents      INTEGER NOT NULL CHECK (amount_cents > 0),
    description       TEXT NOT NULL DEFAULT '',
    category_id       INTEGER NOT NULL REFERENCES categories (id) ON DELETE RESTRICT,
    recurring_item_id INTEGER REFERENCES recurring_items (id) ON DELETE SET NULL,
    scheduled_date    TEXT
);

CREATE INDEX transactions_date ON transactions (date);
CREATE INDEX transactions_category ON transactions (category_id);
CREATE INDEX transactions_recurring ON transactions (recurring_item_id);
CREATE UNIQUE INDEX transactions_one_per_schedule_date
    ON transactions (recurring_item_id, scheduled_date)
    WHERE recurring_item_id IS NOT NULL AND scheduled_date IS NOT NULL;

CREATE TABLE budgets (
    id           INTEGER PRIMARY KEY,
    month        TEXT NOT NULL,
    category_id  INTEGER NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
    amount_cents INTEGER NOT NULL CHECK (amount_cents >= 0),
    UNIQUE (month, category_id)
);
