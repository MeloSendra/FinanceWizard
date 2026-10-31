use crate::categories::{self, Kind};
use crate::dates::parse_date;
use crate::error::{AppError, Result, invalid};
use crate::recurring::{self, RecurringItem};
use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub id: i64,
    pub date: String,
    pub amount_cents: i64,
    pub description: String,
    pub category_id: i64,
    pub category_name: String,
    pub kind: Kind,
    pub recurring_item_id: Option<i64>,
    pub recurring_item_name: Option<String>,
    pub scheduled_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInput {
    pub date: String,
    pub amount_cents: i64,
    pub description: String,
    pub category_id: i64,
    pub recurring_item_id: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionFilter {
    pub from: Option<String>,
    pub to: Option<String>,
    pub kind: Option<Kind>,
    /// Matches the category and all its subcategories.
    pub category_id: Option<i64>,
}

const SELECT: &str = "
    SELECT t.id, t.date, t.amount_cents, t.description, t.category_id, c.name, c.kind,
           t.recurring_item_id, r.name, t.scheduled_date
    FROM transactions t
    JOIN categories c ON c.id = t.category_id
    LEFT JOIN recurring_items r ON r.id = t.recurring_item_id";

fn from_row(row: &Row) -> rusqlite::Result<Transaction> {
    Ok(Transaction {
        id: row.get(0)?,
        date: row.get(1)?,
        amount_cents: row.get(2)?,
        description: row.get(3)?,
        category_id: row.get(4)?,
        category_name: row.get(5)?,
        kind: row.get(6)?,
        recurring_item_id: row.get(7)?,
        recurring_item_name: row.get(8)?,
        scheduled_date: row.get(9)?,
    })
}

pub fn list(conn: &Connection, filter: &TransactionFilter) -> Result<Vec<Transaction>> {
    if let Some(from) = &filter.from {
        parse_date(from)?;
    }
    if let Some(to) = &filter.to {
        parse_date(to)?;
    }
    let sql = format!(
        "WITH RECURSIVE tree(id) AS (
             SELECT ?4
             UNION
             SELECT c.id FROM categories c JOIN tree ON c.parent_id = tree.id
         )
         {SELECT}
         WHERE (?1 IS NULL OR t.date >= ?1)
           AND (?2 IS NULL OR t.date <= ?2)
           AND (?3 IS NULL OR c.kind = ?3)
           AND (?4 IS NULL OR t.category_id IN (SELECT id FROM tree))
         ORDER BY t.date DESC, t.id DESC"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(
        params![filter.from, filter.to, filter.kind, filter.category_id],
        from_row,
    )?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Transaction> {
    conn.query_row(&format!("{SELECT} WHERE t.id = ?1"), [id], from_row)
        .optional()?
        .ok_or(AppError::NotFound)
}

fn validate(conn: &Connection, input: &TransactionInput, current_category: Option<i64>) -> Result<()> {
    parse_date(&input.date)?;
    if input.amount_cents <= 0 {
        return invalid("The amount must be greater than zero.");
    }
    let category = categories::get(conn, input.category_id)?;
    if category.archived && current_category != Some(category.id) {
        return invalid("Archived categories cannot be used for new entries.");
    }
    if input.recurring_item_id.is_some() && category.kind != Kind::Expense {
        return invalid("Recurring payments must use an expense category.");
    }
    Ok(())
}

pub fn create(conn: &Connection, input: &TransactionInput) -> Result<Transaction> {
    validate(conn, input, None)?;
    conn.execute(
        "INSERT INTO transactions (date, amount_cents, description, category_id, recurring_item_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![input.date, input.amount_cents, input.description.trim(), input.category_id, input.recurring_item_id],
    )?;
    get(conn, conn.last_insert_rowid())
}

/// The recurring item a payment counts toward, read before the payment changes.
fn linked_item(conn: &Connection, transaction: &Transaction, today: NaiveDate) -> Result<Option<RecurringItem>> {
    transaction.recurring_item_id.map(|id| recurring::get(conn, id, today)).transpose()
}

pub fn update(conn: &Connection, id: i64, input: &TransactionInput, today: NaiveDate) -> Result<Transaction> {
    let existing = get(conn, id)?;
    validate(conn, input, Some(existing.category_id))?;
    let linked = linked_item(conn, &existing, today)?;
    conn.execute(
        "UPDATE transactions
         SET date = ?1, amount_cents = ?2, description = ?3, category_id = ?4, recurring_item_id = ?5
         WHERE id = ?6",
        params![input.date, input.amount_cents, input.description.trim(), input.category_id, input.recurring_item_id, id],
    )?;
    if let Some(item) = linked {
        recurring::reopen_if_unfinished(conn, &item, today)?;
    }
    get(conn, id)
}

pub fn delete(conn: &Connection, id: i64, today: NaiveDate) -> Result<()> {
    let linked = linked_item(conn, &get(conn, id)?, today)?;
    conn.execute("DELETE FROM transactions WHERE id = ?1", [id])?;
    if let Some(item) = linked {
        recurring::reopen_if_unfinished(conn, &item, today)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::{CategoryInput, Kind};
    use crate::db::test_connection;

    fn category(conn: &Connection, name: &str, kind: Kind, parent_id: Option<i64>) -> i64 {
        let input = CategoryInput { name: name.into(), kind, parent_id, default_budget_cents: None, color: None };
        categories::create(conn, &input).unwrap().id
    }

    fn input(date: &str, amount_cents: i64, category_id: i64) -> TransactionInput {
        TransactionInput { date: date.into(), amount_cents, description: String::new(), category_id, recurring_item_id: None }
    }

    #[test]
    fn rejects_invalid_amounts_and_dates() {
        let conn = test_connection();
        let food = category(&conn, "Food", Kind::Expense, None);
        assert!(create(&conn, &input("2026-01-01", 0, food)).is_err());
        assert!(create(&conn, &input("2026-01-01", -5, food)).is_err());
        assert!(create(&conn, &input("2026-13-01", 100, food)).is_err());
    }

    #[test]
    fn the_transaction_kind_comes_from_its_category() {
        let conn = test_connection();
        let salary = category(&conn, "Salary", Kind::Income, None);
        let created = create(&conn, &input("2026-01-01", 100, salary)).unwrap();
        assert_eq!(created.kind, Kind::Income);
    }

    #[test]
    fn archived_categories_are_rejected_but_existing_entries_stay_editable() {
        let conn = test_connection();
        let food = category(&conn, "Food", Kind::Expense, None);
        let existing = create(&conn, &input("2026-01-01", 100, food)).unwrap();
        categories::set_archived(&conn, food, true).unwrap();
        assert!(create(&conn, &input("2026-01-02", 100, food)).is_err());
        let today = crate::dates::parse_date("2026-01-03").unwrap();
        update(&conn, existing.id, &input("2026-01-03", 200, food), today).unwrap();
    }

    #[test]
    fn filters_by_range_kind_and_category_tree() {
        let conn = test_connection();
        let food = category(&conn, "Food", Kind::Expense, None);
        let snacks = category(&conn, "Snacks", Kind::Expense, Some(food));
        let salary = category(&conn, "Salary", Kind::Income, None);
        create(&conn, &input("2026-01-31", 100, food)).unwrap();
        create(&conn, &input("2026-02-01", 200, snacks)).unwrap();
        create(&conn, &input("2026-02-28", 300, salary)).unwrap();
        create(&conn, &input("2026-03-01", 400, food)).unwrap();

        let february = TransactionFilter { from: Some("2026-02-01".into()), to: Some("2026-02-28".into()), ..Default::default() };
        assert_eq!(list(&conn, &february).unwrap().len(), 2);
        let expenses = TransactionFilter { kind: Some(Kind::Expense), ..Default::default() };
        assert_eq!(list(&conn, &expenses).unwrap().len(), 3);
        let tree = TransactionFilter { category_id: Some(food), ..Default::default() };
        assert_eq!(list(&conn, &tree).unwrap().len(), 3);
    }
}
