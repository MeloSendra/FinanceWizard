use crate::error::{Result, invalid};
use chrono::{Datelike, Days, Months, NaiveDate};

pub fn parse_date(value: &str) -> Result<NaiveDate> {
    match NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        Ok(date) if value.len() == 10 => Ok(date),
        _ => invalid(format!("'{value}' is not a valid date.")),
    }
}

/// First and last day of a `YYYY-MM` month.
pub fn month_bounds(month: &str) -> Result<(NaiveDate, NaiveDate)> {
    let first = parse_date(&format!("{month}-01"))?;
    Ok((first, last_of_month(first)))
}

pub fn last_of_month(date: NaiveDate) -> NaiveDate {
    let first = date.with_day(1).expect("day 1 always exists");
    first + Months::new(1) - Days::new(1)
}

/// The given day in the month `month_index` (year * 12 + zero-based month),
/// moved back to the last day when the month is shorter.
pub fn clamped_date(month_index: i32, day: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(month_index.div_euclid(12), month_index.rem_euclid(12) as u32 + 1, 1)
        .expect("month index comes from a valid date");
    let last = last_of_month(first);
    first.with_day(day.min(last.day())).expect("clamped day exists")
}

pub fn month_index(date: NaiveDate) -> i32 {
    date.year() * 12 + date.month0() as i32
}

pub fn month_key(month_index: i32) -> String {
    format!("{:04}-{:02}", month_index.div_euclid(12), month_index.rem_euclid(12) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dates_that_are_not_iso() {
        assert!(parse_date("2026-02-30").is_err());
        assert!(parse_date("2026-2-3").is_err());
        assert!(parse_date("03/02/2026").is_err());
        assert!(parse_date("2026-02-03").is_ok());
    }

    #[test]
    fn month_bounds_handle_leap_years() {
        let (first, last) = month_bounds("2028-02").unwrap();
        assert_eq!(first.to_string(), "2028-02-01");
        assert_eq!(last.to_string(), "2028-02-29");
    }

    #[test]
    fn day_31_is_clamped_to_the_end_of_short_months() {
        let index = 2026 * 12 + 1;
        assert_eq!(clamped_date(index, 31).to_string(), "2026-02-28");
    }

    #[test]
    fn month_keys_roll_over_the_year() {
        assert_eq!(month_key(2026 * 12 + 11), "2026-12");
        assert_eq!(month_key(2027 * 12), "2027-01");
    }
}
