use crate::dates::parse_date;
use crate::error::{Result, invalid};
use rusqlite::{Connection, params};

fn check_range(from: &str, to: &str) -> Result<()> {
    if parse_date(from)? > parse_date(to)? {
        return invalid("The start date cannot be after the end date.");
    }
    Ok(())
}

pub fn count_transactions(conn: &Connection, from: &str, to: &str) -> Result<i64> {
    check_range(from, to)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM transactions WHERE date BETWEEN ?1 AND ?2",
        params![from, to],
        |row| row.get(0),
    )?)
}

/// Recurring items and budgets stay. Removed scheduled payments are not created
/// again, but they do show up as missed payments on their item.
pub fn delete_transactions(conn: &Connection, from: &str, to: &str) -> Result<usize> {
    check_range(from, to)?;
    Ok(conn.execute("DELETE FROM transactions WHERE date BETWEEN ?1 AND ?2", params![from, to])?)
}

/// Removes everything entered in the app. Settings stay, and so does the vault,
/// which is not in the database.
pub fn reset_all(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DELETE FROM transactions;
         DELETE FROM recurring_value_history;
         DELETE FROM recurring_items;
         DELETE FROM budgets;",
    )?;
    // Parents cannot go while they have children, so this works up from the leaves.
    while tx.execute(
        "DELETE FROM categories WHERE id NOT IN (SELECT parent_id FROM categories WHERE parent_id IS NOT NULL)",
        [],
    )? > 0
    {}
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup;
    use crate::categories::{self, CategoryInput, Kind};
    use crate::db::{self, test_connection, tests::TempDir};
    use crate::recurring::{self, Frequency, ItemType, RecurringInput, Status};
    use crate::reports;
    use crate::transactions::{self, TransactionFilter, TransactionInput};
    use chrono::NaiveDate;

    const KEY: &str = "x'00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff'";

    fn category(conn: &Connection, name: &str, parent_id: Option<i64>) -> i64 {
        let input = CategoryInput { name: name.into(), kind: Kind::Expense, parent_id, default_budget_cents: None, color: None };
        categories::create(conn, &input).unwrap().id
    }

    fn spend(conn: &Connection, date: &str, category_id: i64) {
        let input = TransactionInput { date: date.into(), amount_cents: 10_00, description: String::new(), category_id, recurring_item_id: None };
        transactions::create(conn, &input).unwrap();
    }

    fn dates(conn: &Connection) -> Vec<String> {
        let mut dates: Vec<_> = transactions::list(conn, &TransactionFilter::default()).unwrap().into_iter().map(|t| t.date).collect();
        dates.sort();
        dates
    }

    fn seed(conn: &mut Connection) {
        let home = category(conn, "Home", None);
        let rent = category(conn, "Rent", Some(home));
        spend(conn, "2026-03-01", rent);
        reports::set_budget(conn, "2026-03", home, 100_00).unwrap();
        let input = RecurringInput {
            name: "Gym".into(),
            item_type: ItemType::Subscription,
            frequency: Frequency::Monthly,
            start_date: "2026-01-01".into(),
            end_date: None,
            status: Status::Active,
            category_id: rent,
            schedule_day: Some(1),
            total_cents: None,
            installment_count: None,
            opening_paid_cents: 0,
            opening_paid_installments: 0,
        };
        recurring::create(conn, &input, 30_00, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()).unwrap();
    }

    #[test]
    fn date_ranges_include_both_ends_and_nothing_else() {
        let conn = test_connection();
        let food = category(&conn, "Food", None);
        for date in ["2026-02-28", "2026-03-01", "2026-03-31", "2026-04-01"] {
            spend(&conn, date, food);
        }
        assert_eq!(count_transactions(&conn, "2026-03-01", "2026-03-31").unwrap(), 2);
        assert_eq!(delete_transactions(&conn, "2026-03-01", "2026-03-31").unwrap(), 2);
        assert_eq!(dates(&conn), ["2026-02-28", "2026-04-01"]);
    }

    #[test]
    fn reversed_or_invalid_ranges_are_refused() {
        let conn = test_connection();
        assert!(delete_transactions(&conn, "2026-04-01", "2026-03-01").is_err());
        assert!(count_transactions(&conn, "soon", "2026-03-01").is_err());
    }

    #[test]
    fn deleting_a_range_keeps_recurring_items_and_budgets() {
        let mut conn = test_connection();
        seed(&mut conn);
        delete_transactions(&conn, "2026-01-01", "2026-12-31").unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(recurring::list(&conn, day).unwrap().len(), 1);
        let budgets: i64 = conn.query_row("SELECT COUNT(*) FROM budgets", [], |row| row.get(0)).unwrap();
        assert_eq!(budgets, 1);
    }

    #[test]
    fn reset_removes_all_entries_but_keeps_settings() {
        let mut conn = test_connection();
        seed(&mut conn);
        backup::set_currency(&conn, "USD").unwrap();
        reset_all(&mut conn).unwrap();
        for table in ["transactions", "recurring_items", "recurring_value_history", "budgets", "categories"] {
            let rows: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap();
            assert_eq!(rows, 0, "{table}");
        }
        assert_eq!(backup::settings(&conn).unwrap().currency, "USD");
    }

    #[test]
    fn reset_handles_categories_nested_before_the_depth_limit() {
        let mut conn = test_connection();
        let top = category(&conn, "Home", None);
        let middle = category(&conn, "Utilities", Some(top));
        conn.execute("INSERT INTO categories (name, kind, parent_id) VALUES ('Power', 'expense', ?1)", [middle]).unwrap();
        reset_all(&mut conn).unwrap();
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0)).unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn a_safety_copy_brings_everything_back_after_a_reset() {
        let dir = TempDir::new("purge-restore");
        let (database, safety, undo) = (dir.0.join("live.sqlite"), dir.0.join("safety.sqlite"), dir.0.join("undo.sqlite"));
        let mut conn = db::open(&database, KEY).unwrap();
        seed(&mut conn);
        db::export_to(&conn, &safety, KEY).unwrap();
        reset_all(&mut conn).unwrap();
        assert!(dates(&conn).is_empty());

        assert!(!backup::needs_passphrase(&safety, KEY).unwrap());
        backup::restore_backup(&mut conn, &database, KEY, &safety, None, &undo).unwrap();
        assert_eq!(dates(&conn), ["2026-03-01"]);
    }
}
