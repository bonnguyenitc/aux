use anyhow::{Context, Result};
use directories::ProjectDirs;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

/// Database wrapper for duet's persistent storage
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

        Ok(db)
    }

    fn db_path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("", "", "duet")
            .context("Could not determine data directory")?;
        Ok(dirs.data_dir().join("duet.db"))
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

            CREATE INDEX IF NOT EXISTS idx_history_played ON history(played_at);
            CREATE INDEX IF NOT EXISTS idx_queue_position ON queue(position);
            CREATE INDEX IF NOT EXISTS idx_search_history_at ON search_history(searched_at);
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
        Ok(db)
    }

    pub fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}
