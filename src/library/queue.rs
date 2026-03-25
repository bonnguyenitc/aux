use anyhow::Result;
use crate::youtube::VideoInfo;
use super::db::Database;

/// Add a video to the end of the queue.
/// Returns `Ok(true)` if added, `Ok(false)` if already in queue (duplicate skipped).
pub fn add_to_queue(db: &Database, video: &VideoInfo) -> Result<bool> {
    let conn = db.connection();

    // ── Duplicate check ─────────────────────────────────────────
    let already_in_queue: i64 = conn.query_row(
        "SELECT COUNT(*) FROM queue WHERE video_id = ?1",
        [&video.id],
        |row| row.get(0),
    )?;

    if already_in_queue > 0 {
        return Ok(false);
    }

    // Get the next position
    let max_pos: i64 = conn
        .query_row("SELECT COALESCE(MAX(position), 0) FROM queue", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO queue (video_id, title, channel, url, duration_secs, position)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            video.id,
            video.title,
            video.channel,
            video.url,
            video.duration.map(|d| d as i64),
            max_pos + 1,
        ],
    )?;
    Ok(true)
}

/// Get the entire queue in order
pub fn get_queue(db: &Database) -> Result<Vec<QueueEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT id, video_id, title, channel, url, duration_secs, position
         FROM queue ORDER BY position ASC",
    )?;

    let entries = stmt
        .query_map([], |row| {
            Ok(QueueEntry {
                id: row.get(0)?,
                video_id: row.get(1)?,
                title: row.get(2)?,
                channel: row.get(3)?,
                url: row.get(4)?,
                duration_secs: row.get(5)?,
                position: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Pop the next video from the queue (removes it)
pub fn pop_next(db: &Database) -> Result<Option<QueueEntry>> {
    let conn = db.connection();

    let entry = conn
        .query_row(
            "SELECT id, video_id, title, channel, url, duration_secs, position
             FROM queue ORDER BY position ASC LIMIT 1",
            [],
            |row| {
                Ok(QueueEntry {
                    id: row.get(0)?,
                    video_id: row.get(1)?,
                    title: row.get(2)?,
                    channel: row.get(3)?,
                    url: row.get(4)?,
                    duration_secs: row.get(5)?,
                    position: row.get(6)?,
                })
            },
        )
        .ok();

    if let Some(ref e) = entry {
        conn.execute("DELETE FROM queue WHERE id = ?1", [e.id])?;
    }

    Ok(entry)
}

/// Clear the entire queue
pub fn clear_queue(db: &Database) -> Result<usize> {
    let conn = db.connection();
    let count = conn.execute("DELETE FROM queue", [])?;
    Ok(count)
}

/// Get queue length
pub fn queue_length(db: &Database) -> Result<usize> {
    let conn = db.connection();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM queue", [], |row| row.get(0))?;
    Ok(count as usize)
}

/// Remove a specific item from the queue by its DB id
pub fn remove_from_queue(db: &Database, id: i64) -> Result<bool> {
    let conn = db.connection();
    let count = conn.execute("DELETE FROM queue WHERE id = ?1", [id])?;
    Ok(count > 0)
}

/// Remove a video from the queue by its YouTube video_id.
/// Returns `true` if a row was deleted.
pub fn remove_from_queue_by_video_id(db: &Database, video_id: &str) -> Result<bool> {
    let conn = db.connection();
    let count = conn.execute("DELETE FROM queue WHERE video_id = ?1", [video_id])?;
    Ok(count > 0)
}

/// Check if a video is already in the queue
pub fn is_in_queue(db: &Database, video_id: &str) -> Result<bool> {
    let conn = db.connection();
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM queue WHERE video_id = ?1",
        [video_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub id: i64,
    pub video_id: String,
    pub title: String,
    pub channel: Option<String>,
    pub url: String,
    pub duration_secs: Option<i64>,
    #[allow(dead_code)] // Stored for future drag-to-reorder support
    pub position: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::db::Database;
    use crate::youtube::VideoInfo;

    fn make_video(id: &str) -> VideoInfo {
        VideoInfo {
            id: id.to_string(),
            title: format!("Title for {}", id),
            channel: Some("Test Channel".to_string()),
            url: format!("https://youtube.com/watch?v={}", id),
            duration: Some(300.0),
            view_count: None,
            thumbnail: None,
            description: None,
        }
    }

    #[test]
    fn add_new_video_returns_true() {
        let db = Database::open_in_memory().expect("in-memory db");
        let video = make_video("abc123");
        let result = add_to_queue(&db, &video).expect("add should succeed");
        assert!(result, "first add should return true");
    }

    #[test]
    fn add_duplicate_returns_false() {
        let db = Database::open_in_memory().expect("in-memory db");
        let video = make_video("abc123");
        add_to_queue(&db, &video).expect("first add");
        let result = add_to_queue(&db, &video).expect("second add should not error");
        assert!(!result, "duplicate add should return false");
    }

    #[test]
    fn duplicate_does_not_increase_queue_length() {
        let db = Database::open_in_memory().expect("in-memory db");
        let video = make_video("abc123");
        add_to_queue(&db, &video).expect("first add");
        add_to_queue(&db, &video).expect("second add");
        let len = queue_length(&db).expect("length");
        assert_eq!(len, 1, "queue should have exactly 1 item after duplicate add");
    }

    #[test]
    fn different_videos_both_added() {
        let db = Database::open_in_memory().expect("in-memory db");
        let v1 = make_video("aaa");
        let v2 = make_video("bbb");
        assert!(add_to_queue(&db, &v1).expect("add v1"));
        assert!(add_to_queue(&db, &v2).expect("add v2"));
        assert_eq!(queue_length(&db).expect("len"), 2);
    }
}
