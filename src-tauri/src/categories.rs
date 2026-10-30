use crate::error::{AppError, Result, invalid};
use rusqlite::{Connection, OptionalExtension, Row, params};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Income,
    Expense,
}

impl ToSql for Kind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            Kind::Income => "income".into(),
            Kind::Expense => "expense".into(),
        })
    }
}

impl FromSql for Kind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "income" => Ok(Kind::Income),
            "expense" => Ok(Kind::Expense),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub kind: Kind,
    pub parent_id: Option<i64>,
    pub default_budget_cents: Option<i64>,
    pub archived: bool,
    /// Only top-level categories have one; subcategories derive theirs from it.
    pub color: Option<String>,
    pub transaction_count: i64,
    pub child_count: i64,
    pub recurring_count: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInput {
    pub name: String,
    pub kind: Kind,
    pub parent_id: Option<i64>,
    pub default_budget_cents: Option<i64>,
    pub color: Option<String>,
}

const SELECT: &str = "
    SELECT c.id, c.name, c.kind, c.parent_id, c.default_budget_cents, c.archived,
           (SELECT COUNT(*) FROM transactions t WHERE t.category_id = c.id),
           (SELECT COUNT(*) FROM categories s WHERE s.parent_id = c.id),
           (SELECT COUNT(*) FROM recurring_items r WHERE r.category_id = c.id),
           c.color
    FROM categories c";

fn from_row(row: &Row) -> rusqlite::Result<Category> {
    Ok(Category {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        parent_id: row.get(3)?,
        default_budget_cents: row.get(4)?,
        archived: row.get(5)?,
        color: row.get(9)?,
        transaction_count: row.get(6)?,
        child_count: row.get(7)?,
        recurring_count: row.get(8)?,
    })
}

pub fn list(conn: &Connection) -> Result<Vec<Category>> {
    let mut statement = conn.prepare(&format!("{SELECT} ORDER BY c.name COLLATE NOCASE"))?;
    let rows = statement.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Category> {
    conn.query_row(&format!("{SELECT} WHERE c.id = ?1"), [id], from_row)
        .optional()?
        .ok_or(AppError::NotFound)
}

/// The category and all its ancestors, starting from the category itself.
fn lineage(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    let mut statement = conn.prepare(
        "WITH RECURSIVE lineage(id, parent_id) AS (
             SELECT id, parent_id FROM categories WHERE id = ?1
             UNION
             SELECT c.id, c.parent_id FROM categories c JOIN lineage l ON c.id = l.parent_id
         )
         SELECT id FROM lineage",
    )?;
    let rows = statement.query_map([id], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn validate(conn: &Connection, id: Option<i64>, input: &CategoryInput) -> Result<String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return invalid("A category needs a name.");
    }
    if input.default_budget_cents.is_some_and(|cents| cents < 0) {
        return invalid("A budget cannot be negative.");
    }
    if let Some(color) = &input.color
        && !(color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit()))
    {
        return invalid("Choose a color in the form #rrggbb.");
    }
    if let Some(parent_id) = input.parent_id {
        let parent = get(conn, parent_id)?;
        if parent.kind != input.kind {
            return invalid("A subcategory must have the same kind as its parent.");
        }
        if parent.archived {
            return invalid("An archived category cannot have new subcategories.");
        }
        if let Some(id) = id
            && lineage(conn, parent_id)?.contains(&id)
        {
            return invalid("A category cannot be moved inside itself.");
        }
        if parent.parent_id.is_some() {
            return invalid("Subcategories cannot have subcategories of their own.");
        }
        if let Some(id) = id
            && get(conn, id)?.child_count > 0
        {
            return invalid("A category with subcategories cannot become a subcategory.");
        }
    }
    let duplicate: Option<i64> = conn
        .query_row(
            "SELECT id FROM categories
             WHERE COALESCE(parent_id, 0) = COALESCE(?1, 0) AND name = ?2 COLLATE NOCASE AND id IS NOT ?3",
            params![input.parent_id, name, id],
            |row| row.get(0),
        )
        .optional()?;
    if duplicate.is_some() {
        return invalid(format!("A category named '{name}' already exists here."));
    }
    Ok(name)
}

/// Subcategories take their color from their parent, so none is stored.
fn stored_color(input: &CategoryInput) -> Option<&str> {
    input.color.as_deref().filter(|_| input.parent_id.is_none())
}

pub fn create(conn: &Connection, input: &CategoryInput) -> Result<Category> {
    let name = validate(conn, None, input)?;
    conn.execute(
        "INSERT INTO categories (name, kind, parent_id, default_budget_cents, color) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, input.kind, input.parent_id, input.default_budget_cents, stored_color(input)],
    )?;
    get(conn, conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, input: &CategoryInput) -> Result<Category> {
    let existing = get(conn, id)?;
    let name = validate(conn, Some(id), input)?;
    if existing.kind != input.kind
        && (existing.transaction_count > 0 || existing.child_count > 0 || existing.recurring_count > 0)
    {
        return invalid("The kind of a category in use cannot be changed.");
    }
    conn.execute(
        "UPDATE categories SET name = ?1, kind = ?2, parent_id = ?3, default_budget_cents = ?4, color = ?5 WHERE id = ?6",
        params![name, input.kind, input.parent_id, input.default_budget_cents, stored_color(input), id],
    )?;
    get(conn, id)
}

/// Archiving includes every descendant; restoring needs an active parent.
pub fn set_archived(conn: &Connection, id: i64, archived: bool) -> Result<Category> {
    let category = get(conn, id)?;
    if !archived
        && let Some(parent_id) = category.parent_id
        && get(conn, parent_id)?.archived
    {
        return invalid("Restore the parent category first.");
    }
    conn.execute(
        "WITH RECURSIVE tree(id) AS (
             SELECT ?1
             UNION
             SELECT c.id FROM categories c JOIN tree t ON c.parent_id = t.id
         )
         UPDATE categories SET archived = ?2
         WHERE id IN (SELECT id FROM tree) AND (?2 = 1 OR id = ?1)",
        params![id, archived],
    )?;
    get(conn, id)
}

/// Deleting is only allowed for unused categories, since removing one with
/// history would change past reports. Those are archived instead.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let category = get(conn, id)?;
    if category.child_count > 0 {
        return invalid("This category has subcategories. Archive it instead.");
    }
    if category.transaction_count > 0 || category.recurring_count > 0 {
        return invalid("This category is in use. Archive it instead, so your history stays intact.");
    }
    conn.execute("DELETE FROM categories WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_connection;

    fn input(name: &str, kind: Kind, parent_id: Option<i64>) -> CategoryInput {
        CategoryInput { name: name.into(), kind, parent_id, default_budget_cents: None, color: None }
    }

    #[test]
    fn rejects_cycles() {
        let conn = test_connection();
        let top = create(&conn, &input("Home", Kind::Expense, None)).unwrap();
        let child = create(&conn, &input("Utilities", Kind::Expense, Some(top.id))).unwrap();
        assert!(update(&conn, top.id, &input("Home", Kind::Expense, Some(child.id))).is_err());
        assert!(update(&conn, top.id, &input("Home", Kind::Expense, Some(top.id))).is_err());
    }

    #[test]
    fn categories_stop_at_two_levels() {
        let conn = test_connection();
        let top = create(&conn, &input("Home", Kind::Expense, None)).unwrap();
        let child = create(&conn, &input("Utilities", Kind::Expense, Some(top.id))).unwrap();
        assert!(create(&conn, &input("Power", Kind::Expense, Some(child.id))).is_err());

        let other = create(&conn, &input("Car", Kind::Expense, None)).unwrap();
        assert!(update(&conn, top.id, &input("Home", Kind::Expense, Some(other.id))).is_err());
        update(&conn, child.id, &input("Utilities", Kind::Expense, Some(other.id))).unwrap();
        update(&conn, child.id, &input("Utilities", Kind::Expense, None)).unwrap();
    }

    #[test]
    fn colors_must_be_hex_and_only_stay_on_parents() {
        let conn = test_connection();
        let colored = |name: &str, parent_id, color: &str| CategoryInput { color: Some(color.into()), ..input(name, Kind::Expense, parent_id) };
        for bad in ["red", "#12345", "#12345g", "123456", "#\u{e9}\u{e9}\u{e9}"] {
            assert!(create(&conn, &colored("Food", None, bad)).is_err(), "{bad}");
        }
        let parent = create(&conn, &colored("Home", None, "#2A78d6")).unwrap();
        assert_eq!(parent.color.as_deref(), Some("#2A78d6"));

        let child = create(&conn, &colored("Rent", Some(parent.id), "#eb6834")).unwrap();
        assert_eq!(child.color, None);
        let moved_under = create(&conn, &colored("Car", None, "#1baf7a")).unwrap();
        let updated = update(&conn, moved_under.id, &colored("Car", Some(parent.id), "#1baf7a")).unwrap();
        assert_eq!(updated.color, None);
    }

    #[test]
    fn subcategories_must_match_their_parent_kind() {
        let conn = test_connection();
        let parent = create(&conn, &input("Salary", Kind::Income, None)).unwrap();
        assert!(create(&conn, &input("Food", Kind::Expense, Some(parent.id))).is_err());
    }

    #[test]
    fn rejects_duplicate_sibling_names_ignoring_case() {
        let conn = test_connection();
        create(&conn, &input("Food", Kind::Expense, None)).unwrap();
        assert!(create(&conn, &input("food", Kind::Expense, None)).is_err());
    }

    #[test]
    fn categories_with_children_cannot_be_deleted() {
        let conn = test_connection();
        let parent = create(&conn, &input("Home", Kind::Expense, None)).unwrap();
        let child = create(&conn, &input("Rent", Kind::Expense, Some(parent.id))).unwrap();
        assert!(delete(&conn, parent.id).is_err());
        delete(&conn, child.id).unwrap();
        delete(&conn, parent.id).unwrap();
    }

    #[test]
    fn categories_with_transactions_cannot_be_deleted_or_change_kind() {
        let conn = test_connection();
        let category = create(&conn, &input("Food", Kind::Expense, None)).unwrap();
        conn.execute(
            "INSERT INTO transactions (date, amount_cents, category_id) VALUES ('2026-01-01', 100, ?1)",
            [category.id],
        )
        .unwrap();
        assert!(delete(&conn, category.id).is_err());
        assert!(update(&conn, category.id, &input("Food", Kind::Income, None)).is_err());
    }

    #[test]
    fn archiving_covers_descendants_and_restoring_needs_the_parent() {
        let conn = test_connection();
        let parent = create(&conn, &input("Home", Kind::Expense, None)).unwrap();
        let child = create(&conn, &input("Rent", Kind::Expense, Some(parent.id))).unwrap();
        set_archived(&conn, parent.id, true).unwrap();
        assert!(get(&conn, child.id).unwrap().archived);
        assert!(set_archived(&conn, child.id, false).is_err());
        set_archived(&conn, parent.id, false).unwrap();
        assert!(!get(&conn, parent.id).unwrap().archived);
        assert!(get(&conn, child.id).unwrap().archived);
    }
}
