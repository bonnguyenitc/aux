use anyhow::{Context, Result};
use directories::ProjectDirs;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

/// Database wrapper for aux's persistent storage
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open or create the database
    pub fn open() -> Result<Self> {
        let path = Self::db_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)
            .with_context(|| format!("Failed to open database: {}", path.display()))?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        db.run_migrations()?;

        Ok(db)
    }

    fn db_path() -> Result<PathBuf> {
        let dirs =
            ProjectDirs::from("", "", "aux").context("Could not determine data directory")?;
        Ok(dirs.data_dir().join("aux.db"))
    }

    /// Run schema migrations (idempotent).
    fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Migration 1: Add source column to content tables
        let tables = ["history", "favorites", "queue", "playlist_items"];
        for table in tables {
            let has_source: bool = conn
                .prepare(&format!("PRAGMA table_info({})", table))?
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .any(|name| name == "source");

            if !has_source {
                conn.execute_batch(&format!(
                    "ALTER TABLE {} ADD COLUMN source TEXT NOT NULL DEFAULT 'youtube';",
                    table
                ))?;
            }
        }

        Ok(())
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                video_id TEXT NOT NULL,
                title TEXT NOT NULL,
                channel TEXT,
                url TEXT NOT NULL,
                duration_secs INTEGER,
                listened_secs INTEGER DEFAULT 0,
                played_at TEXT DEFAULT (datetime('now', 'localtime'))
            );

            CREATE TABLE IF NOT EXISTS favorites (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                video_id TEXT UNIQUE NOT NULL,
                title TEXT NOT NULL,
                channel TEXT,
                url TEXT NOT NULL,
                duration_secs INTEGER,
                added_at TEXT DEFAULT (datetime('now', 'localtime'))
            );

            CREATE TABLE IF NOT EXISTS queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                video_id TEXT NOT NULL,
                title TEXT NOT NULL,
                channel TEXT,
                url TEXT NOT NULL,
                duration_secs INTEGER,
                position INTEGER NOT NULL,
                added_at TEXT DEFAULT (datetime('now', 'localtime'))
            );

            CREATE TABLE IF NOT EXISTS search_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                query TEXT NOT NULL UNIQUE,
                searched_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
            );

            CREATE TABLE IF NOT EXISTS playlists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                created_at TEXT DEFAULT (datetime('now', 'localtime'))
            );

            CREATE TABLE IF NOT EXISTS playlist_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                playlist_id INTEGER NOT NULL,
                video_id TEXT NOT NULL,
                title TEXT NOT NULL,
                channel TEXT,
                url TEXT NOT NULL,
                duration_secs INTEGER,
                position INTEGER NOT NULL,
                added_at TEXT DEFAULT (datetime('now', 'localtime')),
                FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
                UNIQUE(playlist_id, video_id)
            );

            CREATE TABLE IF NOT EXISTS playback_positions (
                video_id TEXT PRIMARY KEY,
                position_secs INTEGER NOT NULL,
                duration_secs INTEGER,
                updated_at TEXT DEFAULT (datetime('now','localtime'))
            );

            CREATE INDEX IF NOT EXISTS idx_history_played ON history(played_at);
            CREATE INDEX IF NOT EXISTS idx_queue_position ON queue(position);
            CREATE INDEX IF NOT EXISTS idx_search_history_at ON search_history(searched_at);
            CREATE INDEX IF NOT EXISTS idx_playlist_items_pos ON playlist_items(playlist_id, position);
            ",
        )
        .context("Failed to initialize database tables")?;

        Ok(())
    }

    /// Open an in-memory database (for tests only)
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        db.run_migrations()?;
        Ok(db)
    }

    pub fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}
