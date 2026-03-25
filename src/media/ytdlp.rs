use anyhow::{bail, Context, Result};
use tokio::process::Command;

use crate::error::DuetError;
use super::source::Source;
use super::types::{StreamUrl, MediaInfo};
use super::MediaBackend;

pub struct YtDlp;

/// Returns true if `input` looks like a URL (any protocol),
/// rather than a keyword search query.
pub fn is_direct_url(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with("https://") || trimmed.starts_with("http://")
}

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

impl YtDlp {
    /// Fetch media info directly from a URL (any platform yt-dlp supports).
    pub async fn fetch_info(&self, url: &str) -> Result<MediaInfo> {
        // Normalise bare video IDs to a full YouTube URL (backward compat)
        let resolved = if !url.contains('.')
            && !url.starts_with("http")
            && url.len() == 11
            && url.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            format!("https://www.youtube.com/watch?v={}", url)
        } else {
            url.to_string()
        };

        let output = Command::new("yt-dlp")
            .args(["--dump-json", "--no-warnings", "--flat-playlist", &resolved])
            .output()
            .await
            .context("Failed to execute yt-dlp")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(DuetError::YtDlpError(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Take only the first JSON line (handles playlists gracefully)
        let first_line = stdout.lines().find(|l| !l.trim().is_empty()).ok_or_else(|| {
            anyhow::anyhow!("yt-dlp returned no output for URL: {}", url)
        })?;

        let mut info: MediaInfo = serde_json::from_str(first_line)
            .context("Failed to parse yt-dlp JSON output")?;

        info.resolve_source();

        Ok(info)
    }
}

impl MediaBackend for YtDlp {
    async fn search(&self, query: &str, limit: usize, source: &Source) -> Result<Vec<MediaInfo>> {
        let prefix = source.search_prefix().ok_or_else(|| {
            anyhow::anyhow!("{} does not support search. Use a direct URL instead.", source)
        })?;

        let search_query = format!("{}{}:{}", prefix, limit, query);

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
            match serde_json::from_str::<MediaInfo>(line) {
                Ok(mut info) => {
                    info.resolve_source();
                    results.push(info);
                }
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
