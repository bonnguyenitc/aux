use anyhow::Result;
use crate::youtube::VideoInfo;
use super::db::Database;

/// Add a video to play history
pub fn add_to_history(db: &Database, video: &VideoInfo, listened_secs: u64) -> Result<()> {
    let conn = db.connection();
    conn.execute(
        "INSERT INTO history (video_id, title, channel, url, duration_secs, listened_secs)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            video.id,
            video.title,
            video.channel,
            video.url,
            video.duration.map(|d| d as i64),
            listened_secs as i64,
        ],
    )?;
    Ok(())
}

/// Get play history
pub fn get_history(db: &Database, limit: usize) -> Result<Vec<HistoryEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel, url, duration_secs, listened_secs, played_at
         FROM history ORDER BY played_at DESC LIMIT ?1",
    )?;

    let entries = stmt
        .query_map([limit as i64], |row| {
            Ok(HistoryEntry {
                video_id: row.get(0)?,
                title: row.get(1)?,
                channel: row.get(2)?,
                url: row.get(3)?,
                duration_secs: row.get(4)?,
                listened_secs: row.get(5)?,
                played_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Get today's history
pub fn get_today_history(db: &Database) -> Result<Vec<HistoryEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel, url, duration_secs, listened_secs, played_at
         FROM history WHERE date(played_at) = date('now', 'localtime')
         ORDER BY played_at DESC",
    )?;

    let entries = stmt
        .query_map([], |row| {
            Ok(HistoryEntry {
                video_id: row.get(0)?,
                title: row.get(1)?,
                channel: row.get(2)?,
                url: row.get(3)?,
                duration_secs: row.get(4)?,
                listened_secs: row.get(5)?,
                played_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub video_id: String,
    pub title: String,
    pub channel: Option<String>,
    pub url: String,
    pub duration_secs: Option<i64>,
    pub listened_secs: i64,
    pub played_at: String,
}
