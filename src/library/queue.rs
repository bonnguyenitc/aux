use anyhow::Result;
use crate::youtube::VideoInfo;
use super::db::Database;

/// Add a video to the end of the queue
pub fn add_to_queue(db: &Database, video: &VideoInfo) -> Result<()> {
    let conn = db.connection();

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
    Ok(())
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

#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub id: i64,
    pub video_id: String,
    pub title: String,
    pub channel: Option<String>,
    pub url: String,
    pub duration_secs: Option<i64>,
    pub position: i64,
}
