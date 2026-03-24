use anyhow::Result;
use super::db::Database;

/// Persist a search query to history.
/// Deduplicates: if the same query already exists it is moved to the top (most recent).
/// # Errors
/// Returns an error if the database operation fails.
pub fn add_search(db: &Database, query: &str) -> Result<()> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(());
    }
    let conn = db.connection();
    // Delete any existing entry for the same query, then insert fresh at the top.
    // We track recency via `searched_at` — ordering by DESC gives most-recent first.
    conn.execute(
        "DELETE FROM search_history WHERE query = ?1",
        [query],
    )?;
    conn.execute(
        "INSERT INTO search_history (query) VALUES (?1)",
        [query],
    )?;
    Ok(())
}

/// Return the most-recent `limit` search queries (newest first).
///
/// # Errors
/// Returns an error if the database operation fails.
pub fn get_searches(db: &Database, limit: usize) -> Result<Vec<String>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT query FROM search_history ORDER BY searched_at DESC LIMIT ?1",
    )?;
    let queries = stmt
        .query_map([limit as i64], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(queries)
}

/// Clear all saved search history.
/// Reserved for a future `duet history clear` sub-command.
///
/// # Errors
/// Returns an error if the database operation fails.
#[allow(dead_code)]
pub fn clear_searches(db: &Database) -> Result<usize> {
    let conn = db.connection();
    let n = conn.execute("DELETE FROM search_history", [])?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::db::Database;

    #[test]
    fn add_and_retrieve() {
        let db = Database::open_in_memory().expect("db");
        add_search(&db, "lofi hip hop").expect("add 1");
        add_search(&db, "jazz piano").expect("add 2");
        let results = get_searches(&db, 10).expect("get");
        assert_eq!(results, vec!["jazz piano", "lofi hip hop"]);
    }

    #[test]
    fn duplicate_is_moved_to_top() {
        let db = Database::open_in_memory().expect("db");
        add_search(&db, "lofi hip hop").expect("add 1");
        add_search(&db, "jazz piano").expect("add 2");
        add_search(&db, "lofi hip hop").expect("readd 1");
        let results = get_searches(&db, 10).expect("get");
        // lofi hip hop should be at the top now, and appear only once
        assert_eq!(results[0], "lofi hip hop");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn empty_query_is_not_saved() {
        let db = Database::open_in_memory().expect("db");
        add_search(&db, "   ").expect("whitespace");
        add_search(&db, "").expect("empty");
        let results = get_searches(&db, 10).expect("get");
        assert!(results.is_empty());
    }

    #[test]
    fn limit_respected() {
        let db = Database::open_in_memory().expect("db");
        for i in 0..10u32 {
            add_search(&db, &format!("query {}", i)).expect("add");
        }
        let results = get_searches(&db, 5).expect("get");
        assert_eq!(results.len(), 5);
    }
}
