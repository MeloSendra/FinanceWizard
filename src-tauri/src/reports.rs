use crate::backup;
use crate::categories::{self, Category, Kind};
use crate::dates::{month_bounds, month_index, month_key};
use crate::error::{Result, invalid};
use crate::recurring::{self, UpcomingPayment};
use chrono::NaiveDate;
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const TREND_MONTHS: i32 = 12;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetRow {
    pub category_id: i64,
    pub name: String,
    pub depth: usize,
    pub budget_cents: Option<i64>,
    pub has_override: bool,
    pub spent_cents: i64,
    pub upcoming_cents: i64,
    pub projected_cents: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub month: String,
    pub income_cents: i64,
    pub expense_cents: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthReport {
    pub month: String,
    pub income_cents: i64,
    pub upcoming_income_cents: i64,
    pub projected_income_cents: i64,
    pub spent_cents: i64,
    pub upcoming_cents: i64,
    pub projected_cents: i64,
    pub budget_cents: Option<i64>,
    pub rows: Vec<BudgetRow>,
    pub upcoming: Vec<UpcomingPayment>,
    pub upcoming_income: Vec<UpcomingPayment>,
    pub trend: Vec<TrendPoint>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavingsMonth {
    pub month: String,
    pub income_cents: i64,
    pub expense_cents: i64,
    pub saved_cents: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavingsReport {
    pub total_cents: i64,
    pub current_month_cents: i64,
    pub months: Vec<SavingsMonth>,
}

fn month_totals(conn: &Connection, month: &str) -> Result<HashMap<i64, i64>> {
    let mut statement = conn.prepare(
        "SELECT category_id, SUM(amount_cents) FROM transactions
         WHERE substr(date, 1, 7) = ?1 GROUP BY category_id",
    )?;
    let rows = statement.query_map([month], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn month_overrides(conn: &Connection, month: &str) -> Result<HashMap<i64, i64>> {
    let mut statement = conn.prepare("SELECT category_id, amount_cents FROM budgets WHERE month = ?1")?;
    let rows = statement.query_map([month], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn ancestors<'a>(by_id: &'a HashMap<i64, &'a Category>, mut id: i64) -> impl Iterator<Item = i64> + 'a {
    std::iter::from_fn(move || {
        let parent = by_id.get(&id)?.parent_id?;
        id = parent;
        Some(parent)
    })
}

/// Adds each category's amount to itself and all its ancestors, so a parent
/// reports the total of its whole branch.
fn roll_up(amounts: &HashMap<i64, i64>, by_id: &HashMap<i64, &Category>) -> HashMap<i64, i64> {
    let mut totals = amounts.clone();
    for (&id, &cents) in amounts {
        for ancestor in ancestors(by_id, id) {
            *totals.entry(ancestor).or_default() += cents;
        }
    }
    totals
}

fn budget_rows(conn: &Connection, month: &str, upcoming: &[UpcomingPayment]) -> Result<(Vec<BudgetRow>, Option<i64>)> {
    let all = categories::list(conn)?;
    let expenses: Vec<&Category> = all.iter().filter(|c| c.kind == Kind::Expense).collect();
    let by_id: HashMap<i64, &Category> = expenses.iter().map(|c| (c.id, *c)).collect();

    let own_spent: HashMap<i64, i64> = month_totals(conn, month)?
        .into_iter()
        .filter(|(id, _)| by_id.contains_key(id))
        .collect();
    let mut own_upcoming: HashMap<i64, i64> = HashMap::new();
    for payment in upcoming {
        *own_upcoming.entry(payment.category_id).or_default() += payment.amount_cents;
    }
    let spent = roll_up(&own_spent, &by_id);
    let scheduled = roll_up(&own_upcoming, &by_id);
    let overrides = month_overrides(conn, month)?;

    let budget_of = |category: &Category| overrides.get(&category.id).copied().or(category.default_budget_cents);

    // A budget inside a branch that already has one is part of that budget,
    // so only the topmost budgets add up to the month total.
    let mut total = None;
    for category in &expenses {
        if let Some(cents) = budget_of(category)
            && !category.archived
            && !ancestors(&by_id, category.id).any(|id| budget_of(by_id[&id]).is_some())
        {
            *total.get_or_insert(0) += cents;
        }
    }

    let mut children: HashMap<Option<i64>, Vec<&Category>> = HashMap::new();
    for category in &expenses {
        children.entry(category.parent_id).or_default().push(category);
    }
    let mut rows = Vec::new();
    let mut stack: Vec<(&Category, usize)> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|category| (*category, 0))
        .collect();
    while let Some((category, depth)) = stack.pop() {
        let spent_cents = spent.get(&category.id).copied().unwrap_or(0);
        let upcoming_cents = scheduled.get(&category.id).copied().unwrap_or(0);
        if !category.archived || spent_cents > 0 || upcoming_cents > 0 {
            rows.push(BudgetRow {
                category_id: category.id,
                name: category.name.clone(),
                depth,
                budget_cents: budget_of(category),
                has_override: overrides.contains_key(&category.id),
                spent_cents,
                upcoming_cents,
                projected_cents: spent_cents + upcoming_cents,
            });
        }
        for child in children.get(&Some(category.id)).into_iter().flatten().rev() {
            stack.push((child, depth + 1));
        }
    }
    Ok((rows, total))
}

/// Income (true) and expense (false) totals by month, for months in the range.
fn monthly_totals(conn: &Connection, first: i32, last: i32) -> Result<HashMap<(String, bool), i64>> {
    let mut statement = conn.prepare(
        "SELECT substr(t.date, 1, 7), c.kind, SUM(t.amount_cents)
         FROM transactions t JOIN categories c ON c.id = t.category_id
         WHERE substr(t.date, 1, 7) BETWEEN ?1 AND ?2
         GROUP BY 1, 2",
    )?;
    let rows = statement.query_map(params![month_key(first), month_key(last)], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Kind>(1)?, row.get::<_, i64>(2)?))
    })?;
    let mut totals = HashMap::new();
    for row in rows {
        let (month, kind, cents) = row?;
        totals.insert((month, kind == Kind::Income), cents);
    }
    Ok(totals)
}

fn trend(conn: &Connection, end_month: i32) -> Result<Vec<TrendPoint>> {
    let first = end_month - TREND_MONTHS + 1;
    let totals = monthly_totals(conn, first, end_month)?;
    let points = (first..=end_month).map(|index| {
        let month = month_key(index);
        TrendPoint {
            income_cents: totals.get(&(month.clone(), true)).copied().unwrap_or(0),
            expense_cents: totals.get(&(month.clone(), false)).copied().unwrap_or(0),
            month,
        }
    });
    Ok(points.collect())
}

pub fn month_report(conn: &Connection, month: &str, today: NaiveDate) -> Result<MonthReport> {
    let (first, last) = month_bounds(month)?;
    let (upcoming_income, upcoming): (Vec<_>, Vec<_>) = recurring::upcoming(conn, today, first, last)?
        .into_iter()
        .partition(|payment| payment.kind == Kind::Income);
    let upcoming_cents = upcoming.iter().map(|payment| payment.amount_cents).sum();
    let upcoming_income_cents: i64 = upcoming_income.iter().map(|payment| payment.amount_cents).sum();
    let (rows, budget_cents) = budget_rows(conn, month, &upcoming)?;

    let expense_ids: HashSet<i64> = categories::list(conn)?
        .iter()
        .filter(|c| c.kind == Kind::Expense)
        .map(|c| c.id)
        .collect();
    let (mut income_cents, mut spent_cents) = (0, 0);
    for (id, cents) in month_totals(conn, month)? {
        if expense_ids.contains(&id) {
            spent_cents += cents;
        } else {
            income_cents += cents;
        }
    }

    Ok(MonthReport {
        month: month.to_string(),
        income_cents,
        upcoming_income_cents,
        projected_income_cents: income_cents + upcoming_income_cents,
        spent_cents,
        upcoming_cents,
        projected_cents: spent_cents + upcoming_cents,
        budget_cents,
        rows,
        upcoming,
        upcoming_income,
        trend: trend(conn, month_index(first))?,
    })
}

/// Savings are worked out from transactions and never stored. A month counts
/// once it has ended, and an overspent month takes from the total.
pub fn savings_report(conn: &Connection, today: NaiveDate) -> Result<SavingsReport> {
    let settings = backup::settings(conn)?;
    let current = month_index(today);
    let first_recorded: Option<String> =
        conn.query_row("SELECT MIN(substr(date, 1, 7)) FROM transactions", [], |row| row.get(0))?;
    let first = match settings.savings_start_month.or(first_recorded) {
        Some(month) => month_index(month_bounds(&month)?.0),
        None => current,
    };
    let totals = monthly_totals(conn, first.min(current), current)?;
    let saved = |index: i32| {
        let month = month_key(index);
        let income_cents = totals.get(&(month.clone(), true)).copied().unwrap_or(0);
        let expense_cents = totals.get(&(month.clone(), false)).copied().unwrap_or(0);
        SavingsMonth { month, income_cents, expense_cents, saved_cents: income_cents - expense_cents }
    };
    let months: Vec<SavingsMonth> = (first..current).map(saved).collect();
    Ok(SavingsReport {
        total_cents: settings.savings_opening_cents + months.iter().map(|month| month.saved_cents).sum::<i64>(),
        current_month_cents: saved(current).saved_cents,
        months,
    })
}

pub fn set_budget(conn: &Connection, month: &str, category_id: i64, amount_cents: i64) -> Result<()> {
    month_bounds(month)?;
    if amount_cents < 0 {
        return invalid("A budget cannot be negative.");
    }
    if categories::get(conn, category_id)?.kind != Kind::Expense {
        return invalid("Budgets apply to expense categories.");
    }
    conn.execute(
        "INSERT INTO budgets (month, category_id, amount_cents) VALUES (?1, ?2, ?3)
         ON CONFLICT (month, category_id) DO UPDATE SET amount_cents = excluded.amount_cents",
        params![month, category_id, amount_cents],
    )?;
    Ok(())
}

pub fn clear_budget(conn: &Connection, month: &str, category_id: i64) -> Result<()> {
    conn.execute("DELETE FROM budgets WHERE month = ?1 AND category_id = ?2", params![month, category_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::CategoryInput;
    use crate::dates::parse_date;
    use crate::db::test_connection;
    use crate::recurring::{Frequency, ItemType, RecurringInput, Status, generate_due};
    use crate::transactions::{self, TransactionInput};

    fn date(value: &str) -> NaiveDate {
        parse_date(value).unwrap()
    }

    fn category(conn: &Connection, name: &str, kind: Kind, parent_id: Option<i64>, budget: Option<i64>) -> i64 {
        let input = CategoryInput { name: name.into(), kind, parent_id, default_budget_cents: budget, color: None };
        categories::create(conn, &input).unwrap().id
    }

    fn spend(conn: &Connection, day: &str, cents: i64, category_id: i64) {
        let input = TransactionInput { date: day.into(), amount_cents: cents, description: String::new(), category_id, recurring_item_id: None };
        transactions::create(conn, &input).unwrap();
    }

    fn schedule(conn: &mut Connection, name: &str, category_id: i64, day: u32, cents: i64, today: &str) {
        let input = RecurringInput {
            name: name.into(),
            item_type: ItemType::Subscription,
            frequency: Frequency::Monthly,
            start_date: "2026-01-01".into(),
            end_date: None,
            status: Status::Active,
            category_id,
            schedule_day: Some(day),
            total_cents: None,
            installment_count: None,
            opening_paid_cents: 0,
            opening_paid_installments: 0,
        };
        recurring::create(conn, &input, cents, date(today)).unwrap();
    }

    fn row<'a>(report: &'a MonthReport, id: i64) -> &'a BudgetRow {
        report.rows.iter().find(|row| row.category_id == id).unwrap()
    }

    #[test]
    fn projected_is_spent_so_far_plus_payments_still_due() {
        let mut conn = test_connection();
        let bills = category(&conn, "Bills", Kind::Expense, None, Some(500_00));
        spend(&conn, "2026-03-05", 300_00, bills);
        schedule(&mut conn, "Gym", bills, 28, 150_00, "2026-03-01");
        generate_due(&mut conn, date("2026-03-10")).unwrap();

        let report = month_report(&conn, "2026-03", date("2026-03-10")).unwrap();
        assert_eq!(report.spent_cents, 300_00);
        assert_eq!(report.upcoming_cents, 150_00);
        assert_eq!(report.projected_cents, 450_00);
        assert_eq!(row(&report, bills).projected_cents, 450_00);
    }

    #[test]
    fn a_generated_payment_moves_from_upcoming_to_spent_without_double_counting() {
        let mut conn = test_connection();
        let bills = category(&conn, "Bills", Kind::Expense, None, None);
        schedule(&mut conn, "Gym", bills, 28, 150_00, "2026-03-01");
        generate_due(&mut conn, date("2026-03-28")).unwrap();
        let report = month_report(&conn, "2026-03", date("2026-03-28")).unwrap();
        assert_eq!((report.spent_cents, report.upcoming_cents, report.projected_cents), (150_00, 0, 150_00));
    }

    #[test]
    fn past_months_show_actuals_and_future_months_only_upcoming() {
        let mut conn = test_connection();
        let bills = category(&conn, "Bills", Kind::Expense, None, None);
        spend(&conn, "2026-02-05", 100_00, bills);
        schedule(&mut conn, "Gym", bills, 28, 150_00, "2026-03-01");
        let past = month_report(&conn, "2026-02", date("2026-03-10")).unwrap();
        assert_eq!((past.spent_cents, past.upcoming_cents), (100_00, 0));
        let future = month_report(&conn, "2026-05", date("2026-03-10")).unwrap();
        assert_eq!((future.spent_cents, future.upcoming_cents), (0, 150_00));
    }

    fn savings_data(conn: &Connection) {
        let food = category(conn, "Food", Kind::Expense, None, None);
        let salary = category(conn, "Salary", Kind::Income, None, None);
        spend(conn, "2026-01-05", 1000_00, salary);
        spend(conn, "2026-01-06", 400_00, food);
        // February has nothing, and March overspends.
        spend(conn, "2026-03-05", 1000_00, salary);
        spend(conn, "2026-03-06", 1100_00, food);
        // April is still running.
        spend(conn, "2026-04-05", 1000_00, salary);
    }

    #[test]
    fn savings_add_up_finished_months_and_overspending_subtracts() {
        let conn = test_connection();
        savings_data(&conn);
        let report = savings_report(&conn, date("2026-04-10")).unwrap();
        let saved: Vec<_> = report.months.iter().map(|m| (m.month.as_str(), m.saved_cents)).collect();
        assert_eq!(saved, [("2026-01", 600_00), ("2026-02", 0), ("2026-03", -100_00)]);
        assert_eq!(report.total_cents, 500_00);
        assert_eq!(report.current_month_cents, 1000_00);
    }

    #[test]
    fn savings_start_from_the_opening_balance_and_chosen_month() {
        let conn = test_connection();
        savings_data(&conn);
        backup::set_savings(&conn, 2000_00, Some("2026-02")).unwrap();
        let report = savings_report(&conn, date("2026-04-10")).unwrap();
        assert_eq!(report.months.len(), 2);
        assert_eq!(report.total_cents, 1900_00);

        backup::set_savings(&conn, 2000_00, None).unwrap();
        assert_eq!(savings_report(&conn, date("2026-04-10")).unwrap().months.len(), 3);
    }

    #[test]
    fn savings_ignore_scheduled_income_and_work_with_no_data() {
        let mut conn = test_connection();
        let empty = savings_report(&conn, date("2026-04-10")).unwrap();
        assert_eq!((empty.total_cents, empty.current_month_cents, empty.months.len()), (0, 0, 0));

        let salary = category(&conn, "Salary", Kind::Income, None, None);
        schedule(&mut conn, "Pay", salary, 25, 2000_00, "2026-04-01");
        assert_eq!(savings_report(&conn, date("2026-04-10")).unwrap().current_month_cents, 0);
    }

    #[test]
    fn a_start_month_in_the_future_counts_nothing() {
        let conn = test_connection();
        savings_data(&conn);
        backup::set_savings(&conn, 50_00, Some("2027-01")).unwrap();
        let report = savings_report(&conn, date("2026-04-10")).unwrap();
        assert!(report.months.is_empty());
        assert_eq!(report.total_cents, 50_00);
    }

    #[test]
    fn expected_income_is_projected_separately_from_spending() {
        let mut conn = test_connection();
        let bills = category(&conn, "Bills", Kind::Expense, None, Some(100_00));
        let salary = category(&conn, "Salary", Kind::Income, None, None);
        schedule(&mut conn, "Gym", bills, 28, 150_00, "2026-03-01");
        schedule(&mut conn, "Pay", salary, 25, 2000_00, "2026-03-01");

        let next = month_report(&conn, "2026-04", date("2026-03-10")).unwrap();
        assert_eq!((next.income_cents, next.upcoming_income_cents, next.projected_income_cents), (0, 2000_00, 2000_00));
        assert_eq!((next.upcoming_cents, next.projected_cents), (150_00, 150_00));
        assert_eq!(next.upcoming.len(), 1);
        assert_eq!(next.upcoming_income[0].name, "Pay");
        assert_eq!(row(&next, bills).projected_cents, 150_00);
    }

    #[test]
    fn income_moves_from_expected_to_received_without_double_counting() {
        let mut conn = test_connection();
        let salary = category(&conn, "Salary", Kind::Income, None, None);
        schedule(&mut conn, "Pay", salary, 25, 2000_00, "2026-03-01");
        generate_due(&mut conn, date("2026-03-25")).unwrap();
        let report = month_report(&conn, "2026-03", date("2026-03-25")).unwrap();
        assert_eq!((report.income_cents, report.upcoming_income_cents, report.projected_income_cents), (2000_00, 0, 2000_00));
    }

    #[test]
    fn parents_include_their_subcategories_and_income_is_separate() {
        let conn = test_connection();
        let home = category(&conn, "Home", Kind::Expense, None, None);
        let rent = category(&conn, "Rent", Kind::Expense, Some(home), None);
        let salary = category(&conn, "Salary", Kind::Income, None, None);
        spend(&conn, "2026-03-01", 40_00, home);
        spend(&conn, "2026-03-02", 60_00, rent);
        spend(&conn, "2026-03-03", 1000_00, salary);
        let report = month_report(&conn, "2026-03", date("2026-03-10")).unwrap();
        assert_eq!(row(&report, home).spent_cents, 100_00);
        assert_eq!(row(&report, rent).spent_cents, 60_00);
        assert_eq!(report.spent_cents, 100_00);
        assert_eq!(report.income_cents, 1000_00);
    }

    #[test]
    fn explicit_budgets_override_the_default_for_that_month_only() {
        let conn = test_connection();
        let food = category(&conn, "Food", Kind::Expense, None, Some(200_00));
        set_budget(&conn, "2026-03", food, 300_00).unwrap();
        let march = month_report(&conn, "2026-03", date("2026-03-10")).unwrap();
        assert_eq!(row(&march, food).budget_cents, Some(300_00));
        assert!(row(&march, food).has_override);
        let april = month_report(&conn, "2026-04", date("2026-03-10")).unwrap();
        assert_eq!(row(&april, food).budget_cents, Some(200_00));
        clear_budget(&conn, "2026-03", food).unwrap();
        let cleared = month_report(&conn, "2026-03", date("2026-03-10")).unwrap();
        assert_eq!(row(&cleared, food).budget_cents, Some(200_00));
    }

    #[test]
    fn the_month_budget_does_not_double_count_nested_budgets() {
        let conn = test_connection();
        let home = category(&conn, "Home", Kind::Expense, None, Some(100_00));
        category(&conn, "Rent", Kind::Expense, Some(home), Some(40_00));
        category(&conn, "Food", Kind::Expense, None, Some(50_00));
        let report = month_report(&conn, "2026-03", date("2026-03-10")).unwrap();
        assert_eq!(report.budget_cents, Some(150_00));
    }

    #[test]
    fn the_trend_covers_twelve_months_ending_at_the_selected_month() {
        let conn = test_connection();
        let food = category(&conn, "Food", Kind::Expense, None, None);
        spend(&conn, "2025-04-15", 10_00, food);
        spend(&conn, "2025-03-15", 99_00, food);
        let report = month_report(&conn, "2026-03", date("2026-03-10")).unwrap();
        assert_eq!(report.trend.len(), 12);
        assert_eq!(report.trend[0].month, "2025-04");
        assert_eq!(report.trend[0].expense_cents, 10_00);
        assert_eq!(report.trend[11].month, "2026-03");
    }
}
