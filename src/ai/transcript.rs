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
    #[allow(dead_code)]
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
    let mut result = dedup_segments(segments);

    // Fill gaps: extend each segment's end to the next segment's start
    // so subtitles stay visible until the next line begins.
    for i in 0..result.len().saturating_sub(1) {
        let next_start = result[i + 1].start;
        if next_start > result[i].end {
            result[i].end = next_start;
        }
    }

    result
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

/// Deduplicate consecutive segments with the same text.
/// When duplicates are found, extend the kept segment's end time to cover the full range.
fn dedup_segments(segments: Vec<TranscriptSegment>) -> Vec<TranscriptSegment> {
    let mut result: Vec<TranscriptSegment> = Vec::new();

    for seg in segments {
        if let Some(last) = result.last_mut() {
            if last.text == seg.text {
                // Extend end time to cover the duplicate's range
                if seg.end > last.end {
                    last.end = seg.end;
                }
                continue;
            }
        }
        result.push(seg);
    }

    result
}

/// Fetch transcript for a YouTube video.
///
/// Tries three strategies in order:
/// 1. Manual subtitles (uploaded by creator) — highest quality
/// 2. Auto-generated subtitles — ASR, usually available
/// 3. Video description fallback — always available, lower quality
///
/// Returns `Ok(None)` only if all three strategies produce nothing.
pub async fn fetch_transcript(video_url: &str) -> Result<Option<Transcript>> {
    // ── Tier 1 & 2: VTT subtitles via yt-dlp ────────────────────────────
    if let Some(t) = fetch_vtt_transcript(video_url).await? {
        return Ok(Some(t));
    }

    // ── Tier 3: description as pseudo-transcript ─────────────────────────
    if let Some(t) = fetch_description_transcript(video_url).await {
        return Ok(Some(t));
    }

    Ok(None)
}

/// Try to fetch VTT subtitles: manual first, then auto-generated.
async fn fetch_vtt_transcript(video_url: &str) -> Result<Option<Transcript>> {
    let temp_dir = std::env::temp_dir().join("duet-subs");

    let output_template = temp_dir.join("sub");
    let output_template_str = output_template.to_str().unwrap_or("sub").to_string();

    // Strategy A: manual subtitles (preferred)
    // Strategy B: auto-generated (fallback)
    // We run both in one yt-dlp call with --write-sub --write-auto-sub;
    // yt-dlp writes manual if present, auto otherwise.
    // Auto-generated subs have lang codes like "en-en", "vi-en", so we use glob patterns.

    // Clean temp dir first to avoid stale .vtt files
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    tokio::fs::create_dir_all(&temp_dir).await?;

    let _output = Command::new("yt-dlp")
        .args([
            "--write-sub",
            "--write-auto-sub",
            "--sub-lang", "en.*,vi.*,en,vi",
            "--sub-format", "vtt",
            "--skip-download",
            "--no-warnings",
            "-o", &output_template_str,
            video_url,
        ])
        .current_dir(&temp_dir)
        .output()
        .await
        .context("Failed to invoke yt-dlp for subtitles")?;

    // NOTE: yt-dlp may exit non-zero if ONE language fails (e.g. 429 rate limit)
    // even when another language succeeded.  Don't bail on exit code — just
    // check whether any .vtt files were actually written.

    // Find the generated .vtt files; prefer manual subs (short lang code like "en")
    // over auto-generated (long code like "en-en").
    let mut candidates: Vec<(String, std::path::PathBuf)> = Vec::new();

    if let Ok(mut entries) = tokio::fs::read_dir(&temp_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "vtt") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let l = stem.rsplit('.').next().unwrap_or("en").to_string();
                    candidates.push((l, path));
                }
            }
        }
    }

    // Sort: shorter lang codes first (manual "en" before auto "en-en")
    candidates.sort_by_key(|(l, _)| l.len());

    let mut vtt_content: Option<String> = None;
    let mut lang = String::from("en");

    for (l, path) in &candidates {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            lang = l.clone();
            vtt_content = Some(content);
            break;
        }
    }

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    match vtt_content {
        Some(content) => {
            let segments = parse_vtt(&content);
            if segments.is_empty() {
                Ok(None)
            } else {
                Ok(Some(Transcript { language: lang, segments }))
            }
        }
        None => Ok(None),
    }
}

/// Fetch video description via yt-dlp and turn it into a single-segment
/// pseudo-transcript.  Returns `None` if the description is absent/empty.
async fn fetch_description_transcript(video_url: &str) -> Option<Transcript> {
    let output = Command::new("yt-dlp")
        .args([
            "--print", "description",
            "--no-warnings",
            "--skip-download",
            video_url,
        ])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let description = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if description.is_empty() || description == "NA" {
        return None;
    }

    // Wrap the whole description as one segment spanning the whole video.
    // Duration 0→∞ is represented by a large sentinel value.
    Some(Transcript {
        language: "description".to_string(),
        segments: vec![TranscriptSegment {
            start: Duration::ZERO,
            end: Duration::from_secs(u32::MAX as u64),
            text: description,
        }],
    })
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SubtitleInfo {
    // For future use with subtitle metadata
}
