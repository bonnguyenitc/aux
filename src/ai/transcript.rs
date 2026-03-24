use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct Transcript {
    pub language: String,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    pub start: Duration,
    pub end: Duration,
    pub text: String,
}

impl Transcript {
    /// Get full transcript as plain text
    pub fn full_text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Get transcript around a specific position (±window_secs)
    pub fn text_around(&self, position: Duration, window_secs: u64) -> String {
        let start = position.as_secs().saturating_sub(window_secs);
        let end = position.as_secs() + window_secs;

        self.segments
            .iter()
            .filter(|s| {
                let seg_start = s.start.as_secs();
                let seg_end = s.end.as_secs();
                seg_start >= start && seg_end <= end
            })
            .map(|s| s.text.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Parse VTT subtitle content into transcript segments
fn parse_vtt(content: &str) -> Vec<TranscriptSegment> {
    let mut segments = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Look for timestamp lines like "00:00:01.000 --> 00:00:04.000"
        if line.contains("-->") {
            let parts: Vec<&str> = line.split("-->").collect();
            if parts.len() == 2 {
                let start = parse_vtt_timestamp(parts[0].trim());
                let end = parse_vtt_timestamp(parts[1].trim());

                // Collect text lines until empty line or next timestamp
                let mut text_lines = Vec::new();
                i += 1;
                while i < lines.len() && !lines[i].trim().is_empty() && !lines[i].contains("-->")
                {
                    let text = lines[i]
                        .trim()
                        // Strip VTT formatting tags
                        .replace("<c>", "")
                        .replace("</c>", "");
                    // Skip lines that are just timestamps within cues
                    if !text.starts_with('<') || text.contains(' ') {
                        let clean = strip_vtt_tags(&text);
                        if !clean.is_empty() {
                            text_lines.push(clean);
                        }
                    }
                    i += 1;
                }

                if let (Some(start), Some(end)) = (start, end) {
                    let text = text_lines.join(" ");
                    if !text.is_empty() {
                        segments.push(TranscriptSegment { start, end, text });
                    }
                }
            }
        }
        i += 1;
    }

    // Deduplicate: VTT auto-subs often repeat the same text
    dedup_segments(segments)
}

/// Remove HTML/VTT tags from text
fn strip_vtt_tags(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result.trim().to_string()
}

/// Parse VTT timestamp "HH:MM:SS.mmm" or "MM:SS.mmm" into Duration
fn parse_vtt_timestamp(ts: &str) -> Option<Duration> {
    let parts: Vec<&str> = ts.split(':').collect();
    match parts.len() {
        3 => {
            let hours: f64 = parts[0].parse().ok()?;
            let minutes: f64 = parts[1].parse().ok()?;
            let seconds: f64 = parts[2].parse().ok()?;
            Some(Duration::from_secs_f64(
                hours * 3600.0 + minutes * 60.0 + seconds,
            ))
        }
        2 => {
            let minutes: f64 = parts[0].parse().ok()?;
            let seconds: f64 = parts[1].parse().ok()?;
            Some(Duration::from_secs_f64(minutes * 60.0 + seconds))
        }
        _ => None,
    }
}

/// Deduplicate consecutive segments with the same text
fn dedup_segments(segments: Vec<TranscriptSegment>) -> Vec<TranscriptSegment> {
    let mut result: Vec<TranscriptSegment> = Vec::new();

    for seg in segments {
        if let Some(last) = result.last() {
            if last.text == seg.text {
                continue; // skip duplicate
            }
        }
        result.push(seg);
    }

    result
}

/// Fetch transcript for a YouTube video using yt-dlp
pub async fn fetch_transcript(video_url: &str) -> Result<Option<Transcript>> {
    // Create temp dir for subtitle files
    let temp_dir = std::env::temp_dir().join("duet-subs");
    std::fs::create_dir_all(&temp_dir)?;

    let output_template = temp_dir.join("sub");

    // Try to get auto-subtitles
    let output = Command::new("yt-dlp")
        .args([
            "--write-auto-sub",
            "--sub-lang",
            "en,vi",
            "--sub-format",
            "vtt",
            "--skip-download",
            "--no-warnings",
            "-o",
            output_template.to_str().unwrap_or("sub"),
            video_url,
        ])
        .current_dir(&temp_dir)
        .output()
        .await
        .context("Failed to fetch subtitles")?;

    if !output.status.success() {
        // Not an error — many videos don't have subtitles
        return Ok(None);
    }

    // Find the generated .vtt file
    let mut vtt_content = None;
    let mut lang = String::from("en");

    if let Ok(entries) = std::fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "vtt") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Extract language from filename (e.g., sub.en.vtt)
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Some(l) = stem.rsplit('.').next() {
                            lang = l.to_string();
                        }
                    }
                    vtt_content = Some(content);
                    break;
                }
            }
        }
    }

    // Cleanup temp files
    let _ = std::fs::remove_dir_all(&temp_dir);

    match vtt_content {
        Some(content) => {
            let segments = parse_vtt(&content);
            if segments.is_empty() {
                Ok(None)
            } else {
                Ok(Some(Transcript {
                    language: lang,
                    segments,
                }))
            }
        }
        None => Ok(None),
    }
}

#[derive(Debug, Deserialize)]
struct SubtitleInfo {
    // For future use with subtitle metadata
}
