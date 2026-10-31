use crate::categories::{self, Kind};
use crate::dates::{clamped_date, month_index, parse_date};
use crate::error::{AppError, Result, invalid};
use chrono::{Days, NaiveDate};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum $name { $($variant),+ }

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(match self { $($name::$variant => $text.into()),+ })
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                match value.as_str()? {
                    $($text => Ok($name::$variant),)+
                    _ => Err(FromSqlError::InvalidType),
                }
            }
        }
    };
}

text_enum!(ItemType { Financing => "financing", Subscription => "subscription" });
text_enum!(Frequency { Monthly => "monthly", Quarterly => "quarterly", Yearly => "yearly" });
text_enum!(Status { Active => "active", Completed => "completed" });

impl Frequency {
    fn months(self) -> i32 {
        match self {
            Frequency::Monthly => 1,
            Frequency::Quarterly => 3,
            Frequency::Yearly => 12,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringItem {
    pub id: i64,
    pub name: String,
    pub item_type: ItemType,
    pub frequency: Frequency,
    pub start_date: String,
    pub end_date: Option<String>,
    pub status: Status,
    pub category_id: i64,
    pub category_name: String,
    pub schedule_day: Option<u32>,
    pub generated_through: String,
    pub total_cents: Option<i64>,
    pub installment_count: Option<i64>,
    pub opening_paid_cents: i64,
    pub opening_paid_installments: i64,
    pub paid_cents: i64,
    pub paid_installments: i64,
    pub current_amount_cents: i64,
    pub next_payment_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringInput {
    pub name: String,
    pub item_type: ItemType,
    pub frequency: Frequency,
    pub start_date: String,
    pub end_date: Option<String>,
    pub status: Status,
    pub category_id: i64,
    pub schedule_day: Option<u32>,
    pub total_cents: Option<i64>,
    pub installment_count: Option<i64>,
    pub opening_paid_cents: i64,
    pub opening_paid_installments: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueRecord {
    pub id: i64,
    pub recurring_item_id: i64,
    pub effective_date: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingPayment {
    pub recurring_item_id: i64,
    pub name: String,
    pub date: String,
    pub amount_cents: i64,
    pub category_id: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissedPayment {
    pub date: String,
    pub amount_cents: i64,
}

const SELECT: &str = "
    SELECT r.id, r.name, r.item_type, r.frequency, r.start_date, r.end_date, r.status,
           r.category_id, c.name, r.schedule_day, r.generated_through, r.total_cents,
           r.installment_count, r.opening_paid_cents, r.opening_paid_installments,
           r.opening_paid_cents + COALESCE((SELECT SUM(t.amount_cents) FROM transactions t WHERE t.recurring_item_id = r.id), 0),
           r.opening_paid_installments + (SELECT COUNT(*) FROM transactions t WHERE t.recurring_item_id = r.id)
    FROM recurring_items r
    JOIN categories c ON c.id = r.category_id";

fn from_row(row: &Row) -> rusqlite::Result<RecurringItem> {
    Ok(RecurringItem {
        id: row.get(0)?,
        name: row.get(1)?,
        item_type: row.get(2)?,
        frequency: row.get(3)?,
        start_date: row.get(4)?,
        end_date: row.get(5)?,
        status: row.get(6)?,
        category_id: row.get(7)?,
        category_name: row.get(8)?,
        schedule_day: row.get(9)?,
        generated_through: row.get(10)?,
        total_cents: row.get(11)?,
        installment_count: row.get(12)?,
        opening_paid_cents: row.get(13)?,
        opening_paid_installments: row.get(14)?,
        paid_cents: row.get(15)?,
        paid_installments: row.get(16)?,
        current_amount_cents: 0,
        next_payment_date: None,
    })
}

fn raw_list(conn: &Connection) -> Result<Vec<RecurringItem>> {
    let mut statement = conn.prepare(&format!("{SELECT} ORDER BY r.name COLLATE NOCASE"))?;
    let rows = statement.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn enrich(conn: &Connection, mut item: RecurringItem, today: NaiveDate) -> Result<RecurringItem> {
    item.current_amount_cents = value_at(conn, item.id, today)?;
    let horizon = today + Days::new(800);
    item.next_payment_date = pending_dates(&item, today, horizon)?.first().map(NaiveDate::to_string);
    Ok(item)
}

pub fn list(conn: &Connection, today: NaiveDate) -> Result<Vec<RecurringItem>> {
    raw_list(conn)?.into_iter().map(|item| enrich(conn, item, today)).collect()
}

pub fn get(conn: &Connection, id: i64, today: NaiveDate) -> Result<RecurringItem> {
    let item = conn
        .query_row(&format!("{SELECT} WHERE r.id = ?1"), [id], from_row)
        .optional()?
        .ok_or(AppError::NotFound)?;
    enrich(conn, item, today)
}

/// The value that applies on a date: the latest record effective by then,
/// or the earliest record for dates before the history starts.
pub fn value_at(conn: &Connection, item_id: i64, date: NaiveDate) -> Result<i64> {
    let applicable: Option<i64> = conn
        .query_row(
            "SELECT amount_cents FROM recurring_value_history
             WHERE recurring_item_id = ?1 AND effective_date <= ?2
             ORDER BY effective_date DESC LIMIT 1",
            params![item_id, date.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    match applicable {
        Some(cents) => Ok(cents),
        None => Ok(conn.query_row(
            "SELECT amount_cents FROM recurring_value_history
             WHERE recurring_item_id = ?1 ORDER BY effective_date LIMIT 1",
            [item_id],
            |row| row.get(0),
        )?),
    }
}

/// Assigned dates in `(after, until]`, within the item's start and end dates.
fn occurrences(item: &RecurringItem, after: NaiveDate, until: NaiveDate) -> Result<Vec<NaiveDate>> {
    let Some(day) = item.schedule_day else {
        return Ok(Vec::new());
    };
    let start = parse_date(&item.start_date)?;
    let end = item.end_date.as_deref().map(parse_date).transpose()?;
    let mut dates = Vec::new();
    let mut index = month_index(start);
    loop {
        let date = clamped_date(index, day);
        if date > until {
            return Ok(dates);
        }
        if date >= start && date > after && end.is_none_or(|end| date <= end) {
            dates.push(date);
        }
        index += item.frequency.months();
    }
}

/// Financing stops generating once every installment has been paid.
fn remaining_installments(item: &RecurringItem) -> Option<usize> {
    match (item.item_type, item.installment_count) {
        (ItemType::Financing, Some(count)) => Some((count - item.paid_installments).max(0) as usize),
        _ => None,
    }
}

/// Scheduled dates after `from` that have not been generated yet.
fn pending_dates(item: &RecurringItem, from: NaiveDate, until: NaiveDate) -> Result<Vec<NaiveDate>> {
    if item.status != Status::Active {
        return Ok(Vec::new());
    }
    let after = from.max(parse_date(&item.generated_through)?);
    let mut dates = occurrences(item, after, until)?;
    if let Some(remaining) = remaining_installments(item) {
        dates.truncate(remaining);
    }
    Ok(dates)
}

pub fn upcoming(conn: &Connection, today: NaiveDate, first: NaiveDate, last: NaiveDate) -> Result<Vec<UpcomingPayment>> {
    let mut payments = Vec::new();
    for item in raw_list(conn)? {
        for date in pending_dates(&item, today, last)? {
            if date >= first {
                payments.push(UpcomingPayment {
                    recurring_item_id: item.id,
                    name: item.name.clone(),
                    date: date.to_string(),
                    amount_cents: value_at(conn, item.id, date)?,
                    category_id: item.category_id,
                });
            }
        }
    }
    payments.sort_by(|a, b| a.date.cmp(&b.date).then(a.name.cmp(&b.name)));
    Ok(payments)
}

fn insert_payment(conn: &Connection, item: &RecurringItem, date: NaiveDate) -> Result<usize> {
    Ok(conn.execute(
        "INSERT OR IGNORE INTO transactions (date, amount_cents, description, category_id, recurring_item_id, scheduled_date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?1)",
        params![date.to_string(), value_at(conn, item.id, date)?, item.name, item.category_id, item.id],
    )?)
}

/// Creates a transaction for every assigned date up to today, then moves the
/// generated-through date forward so deleted payments are not created again.
/// Returns how many transactions were created.
pub fn generate_due(conn: &mut Connection, today: NaiveDate) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut created = 0;
    for item in raw_list(&tx)? {
        if item.status != Status::Active {
            continue;
        }
        let from = parse_date(&item.generated_through)?;
        for date in pending_dates(&item, from, today)? {
            created += insert_payment(&tx, &item, date)?;
        }
        tx.execute(
            "UPDATE recurring_items SET generated_through = MAX(generated_through, ?1) WHERE id = ?2",
            params![today.to_string(), item.id],
        )?;
    }
    complete_finished(&tx, today)?;
    tx.commit()?;
    Ok(created)
}

/// Assigned dates that generation has passed without a payment on record, such
/// as those before an item was entered. Installments entered as already paid
/// cover the earliest dates, and a payment linked by hand counts on its own date.
pub fn missed_payments(conn: &Connection, id: i64, today: NaiveDate) -> Result<Vec<MissedPayment>> {
    let item = get(conn, id, today)?;
    let start = parse_date(&item.start_date)?;
    let through = parse_date(&item.generated_through)?;
    let mut statement = conn.prepare(
        "SELECT date FROM transactions WHERE recurring_item_id = ?1
         UNION
         SELECT scheduled_date FROM transactions WHERE recurring_item_id = ?1 AND scheduled_date IS NOT NULL",
    )?;
    let recorded = statement.query_map([id], |row| row.get(0))?.collect::<rusqlite::Result<HashSet<String>>>()?;
    let covered = match item.item_type {
        ItemType::Financing => item.opening_paid_installments as usize,
        ItemType::Subscription => 0,
    };
    let mut dates: Vec<NaiveDate> = occurrences(&item, start - Days::new(1), through)?
        .into_iter()
        .skip(covered)
        .filter(|date| !recorded.contains(&date.to_string()))
        .collect();
    if let Some(remaining) = remaining_installments(&item) {
        dates.truncate(remaining);
    }
    dates
        .into_iter()
        .map(|date| Ok(MissedPayment { date: date.to_string(), amount_cents: value_at(conn, id, date)? }))
        .collect()
}

/// Records the chosen missed payments. Returns how many transactions were created.
pub fn add_missed_payments(conn: &mut Connection, id: i64, dates: &[String], today: NaiveDate) -> Result<usize> {
    let tx = conn.transaction()?;
    let item = get(&tx, id, today)?;
    let missed = missed_payments(&tx, id, today)?;
    let mut created = 0;
    for date in dates {
        if !missed.iter().any(|payment| payment.date == *date) {
            return invalid("That payment is no longer missing.");
        }
        created += insert_payment(&tx, &item, parse_date(date)?)?;
    }
    complete_finished(&tx, today)?;
    tx.commit()?;
    Ok(created)
}

/// Whether the item has run its course, by the same rules as `complete_finished`.
fn finished(item: &RecurringItem, today: NaiveDate) -> bool {
    let paid_off = item.item_type == ItemType::Financing
        && item.installment_count.is_some_and(|count| item.paid_installments >= count);
    paid_off || item.end_date.as_deref().is_some_and(|end| end < today.to_string().as_str())
}

/// Gives back an item that a lost payment has left unfinished. `before` is the
/// item as it was, so one the user completed by hand stays completed. Dates
/// already passed are offered as past payments rather than posted again.
pub fn reopen_if_unfinished(conn: &Connection, before: &RecurringItem, today: NaiveDate) -> Result<()> {
    if before.status == Status::Completed && finished(before, today) && !finished(&get(conn, before.id, today)?, today) {
        conn.execute(
            "UPDATE recurring_items SET status = 'active', generated_through = MAX(generated_through, ?1) WHERE id = ?2",
            params![(today - Days::new(1)).to_string(), before.id],
        )?;
    }
    Ok(())
}

fn complete_finished(conn: &Connection, today: NaiveDate) -> Result<()> {
    conn.execute(
        "UPDATE recurring_items SET status = 'completed'
         WHERE status = 'active' AND end_date IS NOT NULL AND end_date < ?1",
        [today.to_string()],
    )?;
    conn.execute(
        "UPDATE recurring_items SET status = 'completed'
         WHERE status = 'active' AND item_type = 'financing'
           AND opening_paid_installments + (SELECT COUNT(*) FROM transactions t WHERE t.recurring_item_id = recurring_items.id) >= installment_count",
        [],
    )?;
    Ok(())
}

fn validate(conn: &Connection, input: &RecurringInput, current_category: Option<i64>) -> Result<String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return invalid("A recurring item needs a name.");
    }
    let start = parse_date(&input.start_date)?;
    if let Some(end) = input.end_date.as_deref().map(parse_date).transpose()?
        && end < start
    {
        return invalid("The end date cannot be before the start date.");
    }
    let category = categories::get(conn, input.category_id)?;
    if category.kind != Kind::Expense {
        return invalid("Recurring items must use an expense category.");
    }
    if category.archived && current_category != Some(category.id) {
        return invalid("Archived categories cannot be used for new entries.");
    }
    if input.schedule_day.is_some_and(|day| !(1..=31).contains(&day)) {
        return invalid("The payment day must be between 1 and 31.");
    }
    if input.item_type == ItemType::Financing {
        let (Some(total), Some(count)) = (input.total_cents, input.installment_count) else {
            return invalid("Financing needs a total amount and a number of installments.");
        };
        if total <= 0 || count <= 0 {
            return invalid("The total and the number of installments must be greater than zero.");
        }
        if input.opening_paid_cents < 0 || input.opening_paid_installments < 0 {
            return invalid("Amounts already paid cannot be negative.");
        }
        if input.opening_paid_cents > total {
            return invalid("The amount already paid cannot exceed the total.");
        }
        if input.opening_paid_installments > count {
            return invalid("The installments already paid cannot exceed the number of installments.");
        }
    }
    Ok(name)
}

/// Fields that only apply to financing are cleared for other types.
fn financing_fields(input: &RecurringInput) -> (Option<i64>, Option<i64>, i64, i64) {
    match input.item_type {
        ItemType::Financing => (input.total_cents, input.installment_count, input.opening_paid_cents, input.opening_paid_installments),
        ItemType::Subscription => (None, None, 0, 0),
    }
}

pub fn create(conn: &mut Connection, input: &RecurringInput, amount_cents: i64, today: NaiveDate) -> Result<RecurringItem> {
    let name = validate(conn, input, None)?;
    if amount_cents <= 0 {
        return invalid("The amount must be greater than zero.");
    }
    let (total, count, opening_cents, opening_installments) = financing_fields(input);
    let tx = conn.transaction()?;
    // Generation starts now, so an item added with an old start date does not
    // recreate payments that happened before it was entered.
    let yesterday = today - Days::new(1);
    tx.execute(
        "INSERT INTO recurring_items (name, item_type, frequency, start_date, end_date, status, category_id, schedule_day,
                                      generated_through, total_cents, installment_count, opening_paid_cents, opening_paid_installments)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            name, input.item_type, input.frequency, input.start_date, input.end_date, input.status,
            input.category_id, input.schedule_day, yesterday.to_string(), total, count, opening_cents, opening_installments
        ],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO recurring_value_history (recurring_item_id, effective_date, amount_cents) VALUES (?1, ?2, ?3)",
        params![id, input.start_date, amount_cents],
    )?;
    tx.commit()?;
    get(conn, id, today)
}

pub fn update(conn: &Connection, id: i64, input: &RecurringInput, today: NaiveDate) -> Result<RecurringItem> {
    let existing = get(conn, id, today)?;
    let name = validate(conn, input, Some(existing.category_id))?;
    let (total, count, opening_cents, opening_installments) = financing_fields(input);
    let edited = RecurringItem {
        item_type: input.item_type,
        end_date: input.end_date.clone(),
        installment_count: count,
        paid_installments: existing.paid_installments - existing.opening_paid_installments + opening_installments,
        ..existing.clone()
    };
    if existing.status == Status::Completed && input.status == Status::Active && finished(&edited, today) {
        return invalid("This item is finished. Raise the number of installments or change the end date to reopen it.");
    }
    // An edit that undoes what finished the item reopens it. One completed by
    // hand with payments left stays completed.
    let status = if finished(&existing, today) && !finished(&edited, today) { Status::Active } else { input.status };
    // Gaining a schedule or being reactivated also starts from now, for the
    // same reason as in `create`.
    let resumed = (existing.schedule_day.is_none() && input.schedule_day.is_some())
        || (existing.status == Status::Completed && status == Status::Active);
    let mut generated_through = existing.generated_through.clone();
    if resumed {
        generated_through = generated_through.max((today - Days::new(1)).to_string());
    }
    conn.execute(
        "UPDATE recurring_items
         SET name = ?1, item_type = ?2, frequency = ?3, start_date = ?4, end_date = ?5, status = ?6, category_id = ?7,
             schedule_day = ?8, generated_through = ?9, total_cents = ?10, installment_count = ?11,
             opening_paid_cents = ?12, opening_paid_installments = ?13
         WHERE id = ?14",
        params![
            name, input.item_type, input.frequency, input.start_date, input.end_date, status,
            input.category_id, input.schedule_day, generated_through, total, count, opening_cents, opening_installments, id
        ],
    )?;
    get(conn, id, today)
}

/// Linked transactions stay as plain transactions.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    match conn.execute("DELETE FROM recurring_items WHERE id = ?1", [id])? {
        0 => Err(AppError::NotFound),
        _ => Ok(()),
    }
}

pub fn list_values(conn: &Connection, item_id: i64) -> Result<Vec<ValueRecord>> {
    let mut statement = conn.prepare(
        "SELECT id, recurring_item_id, effective_date, amount_cents
         FROM recurring_value_history WHERE recurring_item_id = ?1 ORDER BY effective_date",
    )?;
    let rows = statement.query_map([item_id], |row| {
        Ok(ValueRecord { id: row.get(0)?, recurring_item_id: row.get(1)?, effective_date: row.get(2)?, amount_cents: row.get(3)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn validate_value(conn: &Connection, item_id: i64, id: Option<i64>, effective_date: &str, amount_cents: i64) -> Result<()> {
    parse_date(effective_date)?;
    if amount_cents <= 0 {
        return invalid("The amount must be greater than zero.");
    }
    let clash: Option<i64> = conn
        .query_row(
            "SELECT id FROM recurring_value_history
             WHERE recurring_item_id = ?1 AND effective_date = ?2 AND id IS NOT ?3",
            params![item_id, effective_date, id],
            |row| row.get(0),
        )
        .optional()?;
    if clash.is_some() {
        return invalid("There is already a value starting on that date.");
    }
    Ok(())
}

pub fn add_value(conn: &Connection, item_id: i64, effective_date: &str, amount_cents: i64) -> Result<()> {
    validate_value(conn, item_id, None, effective_date, amount_cents)?;
    conn.execute(
        "INSERT INTO recurring_value_history (recurring_item_id, effective_date, amount_cents) VALUES (?1, ?2, ?3)",
        params![item_id, effective_date, amount_cents],
    )?;
    Ok(())
}

pub fn update_value(conn: &Connection, id: i64, effective_date: &str, amount_cents: i64) -> Result<()> {
    let item_id: i64 = conn
        .query_row("SELECT recurring_item_id FROM recurring_value_history WHERE id = ?1", [id], |row| row.get(0))
        .optional()?
        .ok_or(AppError::NotFound)?;
    validate_value(conn, item_id, Some(id), effective_date, amount_cents)?;
    conn.execute(
        "UPDATE recurring_value_history SET effective_date = ?1, amount_cents = ?2 WHERE id = ?3",
        params![effective_date, amount_cents, id],
    )?;
    Ok(())
}

pub fn delete_value(conn: &Connection, id: i64) -> Result<()> {
    let remaining: i64 = conn.query_row(
        "SELECT COUNT(*) FROM recurring_value_history
         WHERE recurring_item_id = (SELECT recurring_item_id FROM recurring_value_history WHERE id = ?1)",
        [id],
        |row| row.get(0),
    )?;
    if remaining == 0 {
        return Err(AppError::NotFound);
    }
    if remaining == 1 {
        return invalid("A recurring item needs at least one value.");
    }
    conn.execute("DELETE FROM recurring_value_history WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::CategoryInput;
    use crate::db::test_connection;
    use crate::transactions::{self, TransactionFilter, TransactionInput};

    fn date(value: &str) -> NaiveDate {
        parse_date(value).unwrap()
    }

    fn expense(conn: &Connection) -> i64 {
        let input = CategoryInput { name: "Bills".into(), kind: Kind::Expense, parent_id: None, default_budget_cents: None, color: None };
        categories::create(conn, &input).unwrap().id
    }

    fn subscription(category_id: i64, start: &str, day: Option<u32>) -> RecurringInput {
        RecurringInput {
            name: "Rent".into(),
            item_type: ItemType::Subscription,
            frequency: Frequency::Monthly,
            start_date: start.into(),
            end_date: None,
            status: Status::Active,
            category_id,
            schedule_day: day,
            total_cents: None,
            installment_count: None,
            opening_paid_cents: 0,
            opening_paid_installments: 0,
        }
    }

    fn financing(category_id: i64, start: &str, count: i64, opening: i64) -> RecurringInput {
        RecurringInput {
            item_type: ItemType::Financing,
            total_cents: Some(count * 200_00),
            installment_count: Some(count),
            opening_paid_cents: opening * 200_00,
            opening_paid_installments: opening,
            ..subscription(category_id, start, Some(1))
        }
    }

    fn payments(conn: &Connection) -> Vec<(String, i64)> {
        let mut rows: Vec<_> = transactions::list(conn, &TransactionFilter::default())
            .unwrap()
            .into_iter()
            .map(|t| (t.date, t.amount_cents))
            .collect();
        rows.sort();
        rows
    }

    #[test]
    fn generates_every_missed_date_on_launch() {
        let mut conn = test_connection();
        let category = expense(&conn);
        create(&mut conn, &subscription(category, "2026-01-01", Some(15)), 500_00, date("2026-01-01")).unwrap();
        assert_eq!(generate_due(&mut conn, date("2026-03-20")).unwrap(), 3);
        assert_eq!(payments(&conn), vec![
            ("2026-01-15".to_string(), 500_00),
            ("2026-02-15".to_string(), 500_00),
            ("2026-03-15".to_string(), 500_00),
        ]);
    }

    #[test]
    fn a_payment_on_today_is_generated_and_only_once() {
        let mut conn = test_connection();
        let category = expense(&conn);
        create(&mut conn, &subscription(category, "2026-01-01", Some(15)), 500_00, date("2026-01-15")).unwrap();
        assert_eq!(generate_due(&mut conn, date("2026-01-15")).unwrap(), 1);
        assert_eq!(generate_due(&mut conn, date("2026-01-15")).unwrap(), 0);
    }

    #[test]
    fn deleted_payments_do_not_come_back() {
        let mut conn = test_connection();
        let category = expense(&conn);
        create(&mut conn, &subscription(category, "2026-01-01", Some(15)), 500_00, date("2026-01-01")).unwrap();
        generate_due(&mut conn, date("2026-01-20")).unwrap();
        let id = transactions::list(&conn, &TransactionFilter::default()).unwrap()[0].id;
        transactions::delete(&conn, id, date("2026-01-20")).unwrap();
        assert_eq!(generate_due(&mut conn, date("2026-02-01")).unwrap(), 0);
        assert!(payments(&conn).is_empty());
    }

    #[test]
    fn items_added_late_do_not_recreate_old_payments() {
        let mut conn = test_connection();
        let category = expense(&conn);
        create(&mut conn, &financing(category, "2024-01-01", 48, 20), 200_00, date("2026-06-10")).unwrap();
        assert_eq!(generate_due(&mut conn, date("2026-06-10")).unwrap(), 0);
        assert_eq!(generate_due(&mut conn, date("2026-07-02")).unwrap(), 1);
    }

    /// A financing started on 9 September and entered on 30 October.
    fn entered_late(conn: &mut Connection, opening: i64) -> RecurringItem {
        let category = expense(conn);
        let input = RecurringInput { schedule_day: Some(9), ..financing(category, "2026-09-09", 12, opening) };
        let item = create(conn, &input, 200_00, date("2026-10-30")).unwrap();
        generate_due(conn, date("2026-10-30")).unwrap();
        item
    }

    fn missed(conn: &Connection, id: i64) -> Vec<String> {
        missed_payments(conn, id, date("2026-10-30")).unwrap().into_iter().map(|p| p.date).collect()
    }

    #[test]
    fn dates_before_an_item_was_entered_are_listed_as_missed() {
        let mut conn = test_connection();
        let item = entered_late(&mut conn, 0);
        assert!(payments(&conn).is_empty());
        assert_eq!(missed(&conn, item.id), ["2026-09-09", "2026-10-09"]);
    }

    #[test]
    fn installments_already_paid_cover_the_earliest_missed_dates() {
        let mut conn = test_connection();
        let item = entered_late(&mut conn, 1);
        assert_eq!(missed(&conn, item.id), ["2026-10-09"]);
    }

    #[test]
    fn a_payment_linked_by_hand_is_not_missed() {
        let mut conn = test_connection();
        let item = entered_late(&mut conn, 0);
        transactions::create(&conn, &TransactionInput {
            date: "2026-09-09".into(),
            amount_cents: 200_00,
            description: String::new(),
            category_id: item.category_id,
            recurring_item_id: Some(item.id),
        }).unwrap();
        assert_eq!(missed(&conn, item.id), ["2026-10-09"]);
    }

    #[test]
    fn adding_missed_payments_records_them_once_and_counts_toward_progress() {
        let mut conn = test_connection();
        let item = entered_late(&mut conn, 0);
        let chosen = ["2026-10-09".to_string()];
        assert_eq!(add_missed_payments(&mut conn, item.id, &chosen, date("2026-10-30")).unwrap(), 1);
        assert_eq!(payments(&conn), vec![("2026-10-09".to_string(), 200_00)]);
        assert_eq!(get(&conn, item.id, date("2026-10-30")).unwrap().paid_installments, 1);
        assert_eq!(missed(&conn, item.id), ["2026-09-09"]);

        assert!(add_missed_payments(&mut conn, item.id, &chosen, date("2026-10-30")).is_err());
        assert_eq!(payments(&conn).len(), 1);
        assert_eq!(generate_due(&mut conn, date("2026-10-30")).unwrap(), 0);
    }

    #[test]
    fn missed_payments_use_the_value_on_their_date() {
        let mut conn = test_connection();
        let item = entered_late(&mut conn, 0);
        add_value(&conn, item.id, "2026-10-01", 250_00).unwrap();
        let amounts: Vec<_> = missed_payments(&conn, item.id, date("2026-10-30")).unwrap().into_iter().map(|p| p.amount_cents).collect();
        assert_eq!(amounts, [200_00, 250_00]);
    }

    #[test]
    fn missed_payments_stop_at_the_installment_count_and_complete_the_item() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let input = RecurringInput { schedule_day: Some(9), ..financing(category, "2026-01-09", 2, 0) };
        let item = create(&mut conn, &input, 200_00, date("2026-10-30")).unwrap();
        generate_due(&mut conn, date("2026-10-30")).unwrap();
        let dates = missed(&conn, item.id);
        assert_eq!(dates, ["2026-01-09", "2026-02-09"]);
        add_missed_payments(&mut conn, item.id, &dates, date("2026-10-30")).unwrap();
        let item = get(&conn, item.id, date("2026-10-30")).unwrap();
        assert_eq!(item.status, Status::Completed);
        assert!(missed(&conn, item.id).is_empty());
    }

    #[test]
    fn items_without_a_schedule_have_no_missed_payments() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let item = create(&mut conn, &subscription(category, "2026-01-01", None), 10_00, date("2026-10-30")).unwrap();
        assert!(missed(&conn, item.id).is_empty());
    }

    #[test]
    fn gaining_a_schedule_does_not_backfill() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let item = create(&mut conn, &subscription(category, "2024-01-01", None), 10_00, date("2024-01-01")).unwrap();
        generate_due(&mut conn, date("2026-06-10")).unwrap();
        update(&conn, item.id, &subscription(category, "2024-01-01", Some(5)), date("2026-06-10")).unwrap();
        assert_eq!(generate_due(&mut conn, date("2026-06-10")).unwrap(), 0);
        assert_eq!(generate_due(&mut conn, date("2026-07-05")).unwrap(), 1);
    }

    #[test]
    fn uses_the_value_that_applies_on_each_date() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let item = create(&mut conn, &subscription(category, "2026-01-01", Some(1)), 500_00, date("2026-01-01")).unwrap();
        add_value(&conn, item.id, "2026-02-15", 510_00).unwrap();
        generate_due(&mut conn, date("2026-03-01")).unwrap();
        assert_eq!(payments(&conn), vec![
            ("2026-01-01".to_string(), 500_00),
            ("2026-02-01".to_string(), 500_00),
            ("2026-03-01".to_string(), 510_00),
        ]);
    }

    #[test]
    fn day_31_falls_on_the_last_day_of_short_months() {
        let mut conn = test_connection();
        let category = expense(&conn);
        create(&mut conn, &subscription(category, "2026-01-01", Some(31)), 10_00, date("2026-01-01")).unwrap();
        generate_due(&mut conn, date("2026-02-28")).unwrap();
        assert_eq!(payments(&conn).last().unwrap().0, "2026-02-28");
    }

    #[test]
    fn quarterly_items_follow_the_start_month() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let input = RecurringInput { frequency: Frequency::Quarterly, ..subscription(category, "2026-02-01", Some(1)) };
        create(&mut conn, &input, 10_00, date("2026-02-01")).unwrap();
        generate_due(&mut conn, date("2026-09-01")).unwrap();
        let dates: Vec<_> = payments(&conn).into_iter().map(|p| p.0).collect();
        assert_eq!(dates, ["2026-02-01", "2026-05-01", "2026-08-01"]);
    }

    #[test]
    fn financing_stops_at_the_installment_count_and_completes() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let item = create(&mut conn, &financing(category, "2026-01-01", 4, 2), 200_00, date("2026-01-01")).unwrap();
        assert_eq!(generate_due(&mut conn, date("2026-12-01")).unwrap(), 2);
        let item = get(&conn, item.id, date("2026-12-01")).unwrap();
        assert_eq!(item.status, Status::Completed);
        assert_eq!(item.paid_installments, 4);
        assert_eq!(item.paid_cents, 800_00);
    }

    /// A financing of 4 installments, 2 entered as paid, finished by two payments.
    fn finished_financing(conn: &mut Connection) -> (RecurringItem, RecurringInput) {
        let category = expense(conn);
        let input = financing(category, "2026-01-01", 4, 2);
        let item = create(conn, &input, 200_00, date("2026-01-01")).unwrap();
        generate_due(conn, date("2026-03-01")).unwrap();
        let item = get(conn, item.id, date("2026-03-01")).unwrap();
        assert_eq!(item.status, Status::Completed);
        (item, RecurringInput { status: Status::Completed, ..input })
    }

    #[test]
    fn raising_the_installment_count_reopens_a_finished_item() {
        let mut conn = test_connection();
        let (item, input) = finished_financing(&mut conn);
        let today = date("2026-03-10");
        let edited = update(&conn, item.id, &RecurringInput { installment_count: Some(6), ..input }, today).unwrap();
        assert_eq!(edited.status, Status::Active);
        assert_eq!(edited.next_payment_date.as_deref(), Some("2026-04-01"));

        generate_due(&mut conn, today).unwrap();
        assert_eq!(get(&conn, item.id, today).unwrap().status, Status::Active);
    }

    #[test]
    fn lowering_the_installments_already_paid_reopens_a_finished_item() {
        let mut conn = test_connection();
        let (item, input) = finished_financing(&mut conn);
        let input = RecurringInput { opening_paid_installments: 1, ..input };
        let edited = update(&conn, item.id, &input, date("2026-03-10")).unwrap();
        assert_eq!(edited.status, Status::Active);
        assert_eq!(edited.paid_installments, 3);
    }

    #[test]
    fn deleting_a_payment_reopens_a_finished_item_and_lists_the_date_as_missed() {
        let mut conn = test_connection();
        let (item, _) = finished_financing(&mut conn);
        let today = date("2026-03-10");
        let payment = transactions::list(&conn, &TransactionFilter::default()).unwrap()[0].id;
        transactions::delete(&conn, payment, today).unwrap();

        assert_eq!(get(&conn, item.id, today).unwrap().status, Status::Active);
        assert_eq!(generate_due(&mut conn, today).unwrap(), 0);
        let missed: Vec<_> = missed_payments(&conn, item.id, today).unwrap().into_iter().map(|p| p.date).collect();
        assert!(missed.contains(&"2026-03-01".to_string()));
    }

    #[test]
    fn unlinking_a_payment_reopens_a_finished_item() {
        let mut conn = test_connection();
        let (item, _) = finished_financing(&mut conn);
        let today = date("2026-03-10");
        let payment = transactions::list(&conn, &TransactionFilter::default()).unwrap().remove(0);
        let input = TransactionInput {
            date: payment.date,
            amount_cents: payment.amount_cents,
            description: payment.description,
            category_id: payment.category_id,
            recurring_item_id: None,
        };
        transactions::update(&conn, payment.id, &input, today).unwrap();
        assert_eq!(get(&conn, item.id, today).unwrap().status, Status::Active);
    }

    #[test]
    fn an_item_completed_by_hand_stays_completed() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let input = financing(category, "2026-01-01", 12, 0);
        let item = create(&mut conn, &input, 200_00, date("2026-01-01")).unwrap();
        generate_due(&mut conn, date("2026-02-01")).unwrap();
        let today = date("2026-02-02");
        let completed = RecurringInput { status: Status::Completed, ..input };
        update(&conn, item.id, &completed, today).unwrap();
        assert_eq!(update(&conn, item.id, &completed, today).unwrap().status, Status::Completed);

        let payment = transactions::list(&conn, &TransactionFilter::default()).unwrap()[0].id;
        transactions::delete(&conn, payment, today).unwrap();
        assert_eq!(get(&conn, item.id, today).unwrap().status, Status::Completed);
    }

    #[test]
    fn reactivating_an_item_that_is_still_finished_is_rejected() {
        let mut conn = test_connection();
        let (item, input) = finished_financing(&mut conn);
        let active = RecurringInput { status: Status::Active, ..input };
        assert!(update(&conn, item.id, &active, date("2026-03-10")).is_err());
    }

    #[test]
    fn extending_a_passed_end_date_reopens_the_item() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let input = RecurringInput { end_date: Some("2026-02-01".into()), ..subscription(category, "2026-01-01", Some(1)) };
        let item = create(&mut conn, &input, 10_00, date("2026-01-01")).unwrap();
        generate_due(&mut conn, date("2026-03-01")).unwrap();
        let today = date("2026-03-01");
        assert_eq!(get(&conn, item.id, today).unwrap().status, Status::Completed);

        let extended = RecurringInput { end_date: None, status: Status::Completed, ..input };
        assert_eq!(update(&conn, item.id, &extended, today).unwrap().status, Status::Active);
    }

    #[test]
    fn progress_counts_the_opening_balance_and_linked_payments() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let item = create(&mut conn, &financing(category, "2026-01-01", 24, 8), 200_00, date("2026-01-01")).unwrap();
        transactions::create(&conn, &TransactionInput {
            date: "2026-01-05".into(),
            amount_cents: 200_00,
            description: String::new(),
            category_id: category,
            recurring_item_id: Some(item.id),
        }).unwrap();
        let item = get(&conn, item.id, date("2026-01-06")).unwrap();
        assert_eq!(item.paid_installments, 9);
        assert_eq!(item.paid_cents, 1800_00);
    }

    #[test]
    fn the_opening_balance_creates_no_transaction() {
        let mut conn = test_connection();
        let category = expense(&conn);
        create(&mut conn, &financing(category, "2026-01-01", 24, 8), 200_00, date("2026-01-01")).unwrap();
        assert!(payments(&conn).is_empty());
    }

    #[test]
    fn interest_is_allowed_because_the_total_is_entered_by_the_user() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let input = RecurringInput { total_cents: Some(5000_00), ..financing(category, "2026-01-01", 24, 0) };
        let item = create(&mut conn, &input, 200_00, date("2026-01-01")).unwrap();
        assert_eq!(item.total_cents, Some(5000_00));
    }

    #[test]
    fn rejects_duplicate_effective_dates_and_removing_the_last_value() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let item = create(&mut conn, &subscription(category, "2026-01-01", None), 10_00, date("2026-01-01")).unwrap();
        assert!(add_value(&conn, item.id, "2026-01-01", 20_00).is_err());
        let only = list_values(&conn, item.id).unwrap().remove(0);
        assert!(delete_value(&conn, only.id).is_err());
    }

    #[test]
    fn upcoming_only_covers_dates_not_generated_yet() {
        let mut conn = test_connection();
        let category = expense(&conn);
        let input = RecurringInput { ..subscription(category, "2026-01-01", Some(10)) };
        create(&mut conn, &input, 100_00, date("2026-01-01")).unwrap();
        let mut late = subscription(category, "2026-01-01", Some(25));
        late.name = "Gym".into();
        create(&mut conn, &late, 30_00, date("2026-01-01")).unwrap();
        generate_due(&mut conn, date("2026-01-15")).unwrap();
        let due = upcoming(&conn, date("2026-01-15"), date("2026-01-01"), date("2026-01-31")).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].name, "Gym");
    }

    #[test]
    fn upcoming_respects_remaining_installments() {
        let mut conn = test_connection();
        let category = expense(&conn);
        create(&mut conn, &financing(category, "2026-01-01", 2, 1), 200_00, date("2026-01-01")).unwrap();
        let due = upcoming(&conn, date("2026-01-01"), date("2026-02-01"), date("2026-06-30")).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].date, "2026-02-01");
    }
}
