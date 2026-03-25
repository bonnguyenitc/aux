use anyhow::Result;
use crate::youtube::VideoInfo;
use super::db::Database;

#[derive(Debug, Clone)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub item_count: usize,
}

#[derive(Debug, Clone)]
pub struct PlaylistItem {
    pub id: i64,
    pub video_id: String,
    pub title: String,
    pub channel: Option<String>,
    pub url: String,
    pub duration_secs: Option<i64>,
    pub position: i64,
}

/// Create a new playlist. Returns the playlist id.
pub fn create_playlist(db: &Database, name: &str) -> Result<i64> {
    let conn = db.connection();
    conn.execute(
        "INSERT INTO playlists (name) VALUES (?1)",
        [name],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Delete a playlist and all its items.
pub fn delete_playlist(db: &Database, name: &str) -> Result<bool> {
    let conn = db.connection();
    // Get playlist id first
    let pid: Option<i64> = conn
        .query_row("SELECT id FROM playlists WHERE name = ?1", [name], |r| r.get(0))
        .ok();
    if let Some(pid) = pid {
        conn.execute("DELETE FROM playlist_items WHERE playlist_id = ?1", [pid])?;
        conn.execute("DELETE FROM playlists WHERE id = ?1", [pid])?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// List all playlists with item counts.
pub fn list_playlists(db: &Database) -> Result<Vec<Playlist>> {
    let conn = db.connection();
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.created_at,
                (SELECT COUNT(*) FROM playlist_items WHERE playlist_id = p.id)
         FROM playlists p ORDER BY p.name ASC",
    )?;

    let playlists = stmt
        .query_map([], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                item_count: row.get::<_, i64>(3)? as usize,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(playlists)
}

/// Add a video to a playlist.
/// Returns Ok(true) if added, Ok(false) if already in playlist.
pub fn add_to_playlist(db: &Database, playlist_name: &str, video: &VideoInfo) -> Result<bool> {
    let conn = db.connection();

    // Find playlist
    let pid: i64 = conn.query_row(
        "SELECT id FROM playlists WHERE name = ?1",
        [playlist_name],
        |r| r.get(0),
    )?;

    // Duplicate check
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM playlist_items WHERE playlist_id = ?1 AND video_id = ?2",
        rusqlite::params![pid, video.id],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Ok(false);
    }

    // Get next position
    let max_pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), 0) FROM playlist_items WHERE playlist_id = ?1",
            [pid],
            |r| r.get(0),
        )
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO playlist_items (playlist_id, video_id, title, channel, url, duration_secs, position)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            pid,
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

/// Remove a video from a playlist by video_id.
pub fn remove_from_playlist(db: &Database, playlist_name: &str, video_id: &str) -> Result<bool> {
    let conn = db.connection();
    let pid: Option<i64> = conn
        .query_row("SELECT id FROM playlists WHERE name = ?1", [playlist_name], |r| r.get(0))
        .ok();
    if let Some(pid) = pid {
        let count = conn.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1 AND video_id = ?2",
            rusqlite::params![pid, video_id],
        )?;
        Ok(count > 0)
    } else {
        Ok(false)
    }
}

/// Get all items in a playlist.
pub fn get_playlist_items(db: &Database, playlist_name: &str) -> Result<Vec<PlaylistItem>> {
    let conn = db.connection();
    let pid: i64 = conn.query_row(
        "SELECT id FROM playlists WHERE name = ?1",
        [playlist_name],
        |r| r.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, video_id, title, channel, url, duration_secs, position
         FROM playlist_items WHERE playlist_id = ?1 ORDER BY position ASC",
    )?;

    let items = stmt
        .query_map([pid], |row| {
            Ok(PlaylistItem {
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

    Ok(items)
}

/// Load all playlist items into the play queue (appending).
/// Returns the number of items added.
pub fn load_playlist_to_queue(db: &Database, playlist_name: &str) -> Result<usize> {
    let items = get_playlist_items(db, playlist_name)?;
    let mut added = 0;
    for item in &items {
        let video = VideoInfo {
            id: item.video_id.clone(),
            title: item.title.clone(),
            channel: item.channel.clone(),
            url: item.url.clone(),
            duration: item.duration_secs.map(|d| d as f64),
            view_count: None,
            thumbnail: None,
            description: None,
        };
        if super::queue::add_to_queue(db, &video)? {
            added += 1;
        }
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::db::Database;
    use crate::youtube::VideoInfo;

    fn video(id: &str) -> VideoInfo {
        VideoInfo {
            id: id.into(),
            title: format!("Video {}", id),
            channel: Some("Channel".into()),
            url: format!("https://youtube.com/watch?v={}", id),
            duration: Some(180.0),
            view_count: None,
            thumbnail: None,
            description: None,
        }
    }

    #[test]
    fn create_and_list() {
        let db = Database::open_in_memory().unwrap();
        create_playlist(&db, "chill").unwrap();
        let pls = list_playlists(&db).unwrap();
        assert_eq!(pls.len(), 1);
        assert_eq!(pls[0].name, "chill");
        assert_eq!(pls[0].item_count, 0);
    }

    #[test]
    fn add_items_and_count() {
        let db = Database::open_in_memory().unwrap();
        create_playlist(&db, "rock").unwrap();
        assert!(add_to_playlist(&db, "rock", &video("a")).unwrap());
        assert!(add_to_playlist(&db, "rock", &video("b")).unwrap());
        // Duplicate
        assert!(!add_to_playlist(&db, "rock", &video("a")).unwrap());

        let pls = list_playlists(&db).unwrap();
        assert_eq!(pls[0].item_count, 2);
    }

    #[test]
    fn delete_playlist_cascades() {
        let db = Database::open_in_memory().unwrap();
        create_playlist(&db, "temp").unwrap();
        add_to_playlist(&db, "temp", &video("x")).unwrap();
        assert!(delete_playlist(&db, "temp").unwrap());
        assert_eq!(list_playlists(&db).unwrap().len(), 0);
    }

    #[test]
    fn load_to_queue() {
        let db = Database::open_in_memory().unwrap();
        create_playlist(&db, "mix").unwrap();
        add_to_playlist(&db, "mix", &video("a")).unwrap();
        add_to_playlist(&db, "mix", &video("b")).unwrap();

        let added = load_playlist_to_queue(&db, "mix").unwrap();
        assert_eq!(added, 2);
        assert_eq!(crate::library::queue::queue_length(&db).unwrap(), 2);
    }
}
