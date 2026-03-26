use super::db::Database;
use anyhow::Result;

/// Save (upsert) the playback position for a video
pub fn save_position(
    db: &Database,
    video_id: &str,
    position_secs: u64,
    duration_secs: u64,
) -> Result<()> {
    let conn = db.connection();
    conn.execute(
        "INSERT INTO playback_positions (video_id, position_secs, duration_secs, updated_at)
         VALUES (?1, ?2, ?3, datetime('now','localtime'))
         ON CONFLICT(video_id) DO UPDATE SET
            position_secs = excluded.position_secs,
            duration_secs = excluded.duration_secs,
            updated_at = excluded.updated_at",
        rusqlite::params![video_id, position_secs as i64, duration_secs as i64],
    )?;
    Ok(())
}

/// Get saved position for a video.
/// Returns None if: no record, listened ≥90%, or remaining <30s.
pub fn get_position(db: &Database, video_id: &str) -> Result<Option<u64>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT position_secs, duration_secs FROM playback_positions WHERE video_id = ?1",
    )?;

    let result = stmt.query_row([video_id], |row| {
        let pos: i64 = row.get(0)?;
        let dur: Option<i64> = row.get(1)?;
        Ok((pos as u64, dur.map(|d| d as u64)))
    });

    match result {
        Ok((position, duration)) => {
            // If no duration info, return the position as-is
            let dur = match duration {
                Some(d) if d > 0 => d,
                _ => return Ok(Some(position)),
            };

            // Threshold: skip resume if ≥90% listened or <30s remaining
            let remaining = dur.saturating_sub(position);
            if position >= dur * 90 / 100 || remaining < 30 {
                // Clear the stale position
                clear_position(db, video_id).ok();
                return Ok(None);
            }

            Ok(Some(position))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Clear saved position for a video (e.g. when track finishes naturally)
pub fn clear_position(db: &Database, video_id: &str) -> Result<()> {
    let conn = db.connection();
    conn.execute(
        "DELETE FROM playback_positions WHERE video_id = ?1",
        [video_id],
    )?;
    Ok(())
}

/// Get all saved positions as a HashMap for display purposes
pub fn get_all_positions(db: &Database) -> Result<std::collections::HashMap<String, u64>> {
    let conn = db.connection();
    let mut stmt = conn.prepare("SELECT video_id, position_secs FROM playback_positions")?;
    let map = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let pos: i64 = row.get(1)?;
            Ok((id, pos as u64))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(map)
}
