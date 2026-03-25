use anyhow::Result;
use crate::media::MediaInfo;
use super::db::Database;

/// Add a video to favorites
pub fn add_favorite(db: &Database, video: &MediaInfo) -> Result<bool> {
    let conn = db.connection();
    let result = conn.execute(
        "INSERT OR IGNORE INTO favorites (video_id, title, channel, url, duration_secs, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            video.id,
            video.title,
            video.channel,
            video.url,
            video.duration.map(|d| d as i64),
            video.source.as_db_str(),
        ],
    )?;
    Ok(result > 0) // true if actually inserted (wasn't already there)
}

/// Remove a video from favorites
pub fn remove_favorite(db: &Database, video_id: &str) -> Result<bool> {
    let conn = db.connection();
    let result = conn.execute(
        "DELETE FROM favorites WHERE video_id = ?1",
        [video_id],
    )?;
    Ok(result > 0)
}

/// Check if a video is in favorites
pub fn is_favorite(db: &Database, video_id: &str) -> Result<bool> {
    let conn = db.connection();
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM favorites WHERE video_id = ?1",
        [video_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Get all favorites
pub fn get_favorites(db: &Database) -> Result<Vec<FavoriteEntry>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel, url, duration_secs, added_at, source
         FROM favorites ORDER BY added_at DESC",
    )?;

    let entries = stmt
        .query_map([], |row| {
            Ok(FavoriteEntry {
                video_id: row.get(0)?,
                title: row.get(1)?,
                channel: row.get(2)?,
                url: row.get(3)?,
                duration_secs: row.get(4)?,
                added_at: row.get(5)?,
                source: row.get::<_, String>(6).unwrap_or_else(|_| "youtube".to_string()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields populated from DB; not all are displayed in current UI
pub struct FavoriteEntry {
    pub video_id: String,
    pub title: String,
    pub channel: Option<String>,
    pub url: String,
    pub duration_secs: Option<i64>,
    pub added_at: String,
    pub source: String,
}
