use anyhow::Result;
use crate::media::MediaInfo;
use super::db::Database;

/// Add a video to play history
pub fn add_to_history(db: &Database, video: &MediaInfo, listened_secs: u64) -> Result<()> {
    let conn = db.connection();
    conn.execute(
        "INSERT INTO history (video_id, title, channel, url, duration_secs, listened_secs, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            video.id,
            video.title,
            video.channel,
            video.url,
            video.duration.map(|d| d as i64),
            listened_secs as i64,
            video.source.as_db_str(),
        ],
    )?;
    Ok(())
}

/// Get play history
pub fn get_history(db: &Database, limit: usize) -> Result<Vec<HistoryEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel, url, duration_secs, listened_secs, played_at, source
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
                source: row.get::<_, String>(7).unwrap_or_else(|_| "youtube".to_string()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Get today's history
pub fn get_today_history(db: &Database) -> Result<Vec<HistoryEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel, url, duration_secs, listened_secs, played_at, source
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
                source: row.get::<_, String>(7).unwrap_or_else(|_| "youtube".to_string()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Get history since a given UTC datetime string (e.g. "2026-03-17 00:00:00")
pub fn get_history_since(db: &Database, since: &str) -> Result<Vec<HistoryEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel, url, duration_secs, listened_secs, played_at, source
         FROM history WHERE played_at >= ?1 ORDER BY played_at DESC",
    )?;
    let entries = stmt
        .query_map([since], |row| {
            Ok(HistoryEntry {
                video_id: row.get(0)?,
                title: row.get(1)?,
                channel: row.get(2)?,
                url: row.get(3)?,
                duration_secs: row.get(4)?,
                listened_secs: row.get(5)?,
                played_at: row.get(6)?,
                source: row.get::<_, String>(7).unwrap_or_else(|_| "youtube".to_string()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// Get all history entries (no date filter, for all-time stats)
pub fn get_all_history(db: &Database) -> Result<Vec<HistoryEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel, url, duration_secs, listened_secs, played_at
         FROM history ORDER BY played_at DESC",
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
                source: row.get::<_, String>(7).unwrap_or_else(|_| "youtube".to_string()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields populated from DB; not all are displayed in current UI
pub struct HistoryEntry {
    pub video_id: String,
    pub title: String,
    pub channel: Option<String>,
    pub url: String,
    pub duration_secs: Option<i64>,
    pub listened_secs: i64,
    pub played_at: String,
    pub source: String,
}
