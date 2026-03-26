use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::types::RepeatMode;
use crate::error::AuxError;
use crate::media::MediaInfo;

const STATE_PATH: &str = "/tmp/aux-state.json";
const PID_PATH: &str = "/tmp/aux.pid";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateFile {
    pub pid: u32,
    pub video: MediaInfo,
    pub started_at: DateTime<Utc>,
    pub speed: f64,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub daemon: bool,
    #[serde(default)]
    pub sleep_deadline: Option<DateTime<Utc>>,
    #[serde(default)]
    pub eq_preset: Option<String>,
}

impl StateFile {
    pub fn state_path() -> PathBuf {
        PathBuf::from(STATE_PATH)
    }

    pub fn pid_path() -> PathBuf {
        PathBuf::from(PID_PATH)
    }

    pub fn new(video: MediaInfo, daemon: bool) -> Self {
        Self {
            pid: std::process::id(),
            video,
            started_at: Utc::now(),
            speed: 1.0,
            repeat: RepeatMode::Off,
            shuffle: false,
            daemon,
            sleep_deadline: None,
            eq_preset: None,
        }
    }

    /// Atomic write: write to tmp file → fsync → rename
    pub fn write(&self) -> Result<()> {
        self.write_to(&Self::state_path())
    }

    pub fn write_to(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("tmp");
        let content = serde_json::to_string_pretty(self)?;
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn read() -> Result<Self> {
        Self::read_from(&Self::state_path())
    }

    pub fn read_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|_| AuxError::NoActiveSession)?;
        serde_json::from_str(&content).context("Corrupt state file")
    }

    pub fn remove() -> Result<()> {
        let _ = std::fs::remove_file(Self::state_path());
        let _ = std::fs::remove_file(Self::pid_path());
        Ok(())
    }

    pub fn exists() -> bool {
        Self::state_path().exists()
    }

    /// Write PID lock file. Returns Err if another instance is running.
    /// Reserved for future daemon mode — guards against duplicate instances.
    #[allow(dead_code)]
    pub fn write_pid_lock() -> Result<()> {
        let path = Self::pid_path();
        if path.exists() {
            let existing = std::fs::read_to_string(&path).unwrap_or_default();
            if let Ok(pid) = existing.trim().parse::<u32>() {
                // Check if process is alive (signal 0 = test existence)
                let alive = unsafe { libc::kill(pid as i32, 0) == 0 };
                if alive {
                    return Err(AuxError::AlreadyRunning { pid }.into());
                }
            }
            // Stale PID file — remove it
            let _ = std::fs::remove_file(&path);
        }
        std::fs::write(&path, std::process::id().to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_video() -> MediaInfo {
        MediaInfo {
            id: "test123".into(),
            title: "Test Video".into(),
            channel: Some("Test Channel".into()),
            duration: Some(300.0),
            view_count: None,
            thumbnail: None,
            description: None,
            url: "https://youtube.com/watch?v=test123".into(),
            source: crate::media::Source::default(),
            extractor_key: None,
        }
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let state = StateFile::new(dummy_video(), false);
        state.write_to(&path).unwrap();
        let loaded = StateFile::read_from(&path).unwrap();
        assert_eq!(loaded.video.id, "test123");
        assert_eq!(loaded.speed, 1.0);
        assert_eq!(loaded.repeat, RepeatMode::Off);
    }

    #[test]
    fn atomic_write_never_corrupts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let state = StateFile::new(dummy_video(), false);
        for _ in 0..50 {
            state.write_to(&path).unwrap();
            let content = std::fs::read_to_string(&path).unwrap();
            serde_json::from_str::<StateFile>(&content)
                .expect("JSON should never be corrupt after atomic write");
        }
    }
}
