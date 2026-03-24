use anyhow::{bail, Context, Result};
use tokio::process::Command;

use crate::error::DuetError;
use super::types::{StreamUrl, VideoInfo};
use super::YouTubeBackend;

pub struct YtDlp;

impl YtDlp {
    pub fn new() -> Self {
        Self
    }

    /// Check if yt-dlp is available in PATH
    pub async fn check_available() -> Result<()> {
        let output = Command::new("yt-dlp")
            .arg("--version")
            .output()
            .await
            .map_err(|_| DuetError::YtDlpNotFound)?;

        if !output.status.success() {
            return Err(DuetError::YtDlpNotFound.into());
        }

        Ok(())
    }
}

impl YouTubeBackend for YtDlp {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<VideoInfo>> {
        let search_query = format!("ytsearch{}:{}", limit, query);

        let output = Command::new("yt-dlp")
            .args([
                "--dump-json",
                "--flat-playlist",
                "--no-warnings",
                &search_query,
            ])
            .output()
            .await
            .context("Failed to execute yt-dlp")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(DuetError::YtDlpError(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();

        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<VideoInfo>(line) {
                Ok(info) => results.push(info),
                Err(e) => {
                    eprintln!("Warning: failed to parse result: {}", e);
                    continue;
                }
            }
        }

        if results.is_empty() {
            bail!(DuetError::NoResults {
                query: query.to_string()
            });
        }

        Ok(results)
    }

    async fn get_stream_url(&self, video_url: &str) -> Result<StreamUrl> {
        let output = Command::new("yt-dlp")
            .args(["-f", "bestaudio", "-g", "--no-warnings", video_url])
            .output()
            .await
            .context("Failed to get stream URL")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(DuetError::YtDlpError(stderr.to_string()));
        }

        let audio_url = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        if audio_url.is_empty() {
            bail!(DuetError::PlaybackError(
                "No audio stream found".to_string()
            ));
        }

        Ok(StreamUrl {
            audio_url,
            format: "m4a".to_string(),
        })
    }
}
