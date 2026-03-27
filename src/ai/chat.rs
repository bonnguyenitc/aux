use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::transcript::Transcript;
use crate::config::ResolvedAiConfig;
use crate::media::MediaInfo;
use crate::player::remote::RemoteSession;
use crate::player::types::RepeatMode;
use crate::player::MediaPlayer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" or "assistant"
    pub content: String,
}

// ── AI-detected player actions ──────────────────────────────────────────────

/// An action the AI detected from the user's natural language message.
/// Named `AiAction` to avoid collision with `player::types::PlayerAction`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiAction {
    // ── Player control ──
    SetVolume { value: u8 },
    Mute,
    Pause,
    Resume,
    Seek { seconds: f64 },
    Next,
    Prev,
    SetSpeed { value: f64 },
    SetRepeat { mode: String },
    ToggleShuffle,
    SetSleep { minutes: u32 },
    CancelSleep,
    // ── Search & play ──
    Search {
        query: String,
        /// Optional source override: "youtube", "soundcloud", "ytmusic".
        /// When omitted the current TUI search source is used.
        #[serde(default)]
        source: Option<String>,
    },
    /// Play a specific item from the current search results (1-based index).
    PlayResult { index: usize },
    /// Pick a random item from the current search results and play it.
    PlayRandom,
    // ── Library management ──
    AddFavorite,
    RemoveFavorite,
    AddToQueue,
    ClearQueue,
    // ── Playlist ──
    CreatePlaylist { name: String },
    DeletePlaylist { name: String },
    AddToPlaylist { playlist: String },
    /// Load a playlist into queue and start playing
    PlayPlaylist { name: String },
    // ── Navigation ──
    /// Switch to a specific TUI panel.
    ShowPanel { panel: String },
}

/// JSON envelope that the LLM always returns.
/// `action` accepts null, a single action object, or an array of actions.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    #[serde(default, deserialize_with = "deserialize_actions")]
    pub action: Vec<AiAction>,
    pub message: String,
}

/// Custom deserializer: accepts null → [], single object → [obj], array → array.
fn deserialize_actions<'de, D>(deserializer: D) -> std::result::Result<Vec<AiAction>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    let val = serde_json::Value::deserialize(deserializer)?;
    match val {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Array(arr) => {
            let mut actions = Vec::with_capacity(arr.len());
            for item in arr {
                let action: AiAction =
                    serde_json::from_value(item).map_err(de::Error::custom)?;
                actions.push(action);
            }
            Ok(actions)
        }
        obj @ serde_json::Value::Object(_) => {
            let action: AiAction =
                serde_json::from_value(obj).map_err(de::Error::custom)?;
            Ok(vec![action])
        }
        _ => Err(de::Error::custom("action must be null, object, or array")),
    }
}

/// Execute an AI-detected player action via RemoteSession.
///
/// `Search` is excluded — it must be handled by the caller since it
/// requires breaking out of the current playback context.
pub async fn execute_action(action: &AiAction) -> Result<()> {
    let remote = RemoteSession::connect()
        .context("no active aux session to execute action")?;

    match action {
        AiAction::SetVolume { value } => remote.set_volume(*value).await?,
        AiAction::Mute => remote.set_volume(0).await?,
        AiAction::Pause => remote.pause().await?,
        AiAction::Resume => remote.resume().await?,
        AiAction::Seek { seconds } => remote.player.seek(*seconds).await?,
        AiAction::Next => {
            remote.player.seek_to(999999.0).await.ok();
        }
        AiAction::Prev => {
            remote.player.seek_to(0.0).await.ok();
        }
        AiAction::SetSpeed { value } => {
            remote.set_speed(value.clamp(0.25, 4.0)).await?;
        }
        AiAction::SetRepeat { mode } => {
            let repeat = match mode.as_str() {
                "one" => RepeatMode::One,
                "all" => RepeatMode::All,
                _ => RepeatMode::Off,
            };
            remote.set_repeat(repeat).await?;
        }
        AiAction::ToggleShuffle => {
            remote.toggle_shuffle().await?;
        }
        AiAction::SetSleep { minutes } => {
            let deadline =
                chrono::Utc::now() + chrono::Duration::minutes(i64::from(*minutes));
            let mut state = crate::player::state::StateFile::read()
                .context("failed to read state for sleep timer")?;
            state.sleep_deadline = Some(deadline);
            state.write()?;
        }
        AiAction::CancelSleep => {
            let mut state = crate::player::state::StateFile::read()
                .context("failed to read state for sleep timer")?;
            state.sleep_deadline = None;
            state.write()?;
        }
        AiAction::Search { .. }
        | AiAction::PlayResult { .. }
        | AiAction::PlayRandom
        | AiAction::AddFavorite
        | AiAction::RemoveFavorite
        | AiAction::AddToQueue
        | AiAction::ClearQueue
        | AiAction::CreatePlaylist { .. }
        | AiAction::DeletePlaylist { .. }
        | AiAction::AddToPlaylist { .. }
        | AiAction::PlayPlaylist { .. }
        | AiAction::ShowPanel { .. } => {
            // Handled by caller (TUI) — these require app state access.
        }
    }
    Ok(())
}

// ── Parse helpers ───────────────────────────────────────────────────────────

/// Try to parse a `ChatResponse` from the raw LLM text.
///
/// Strategy (in order):
/// 1. Direct parse of trimmed text
/// 2. Strip markdown code fences (```json ... ```)
/// 3. Brace-matching: find the outermost `{ ... }` in the text
/// 4. Fallback: raw text becomes the message with no actions
fn parse_chat_response(raw: &str) -> ChatResponse {
    let trimmed = raw.trim();

    // 1. Direct parse
    if let Ok(resp) = serde_json::from_str::<ChatResponse>(trimmed) {
        return resp;
    }

    // 2. Strip markdown code fences
    if trimmed.starts_with("```") {
        let stripped = trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        if let Ok(resp) = serde_json::from_str::<ChatResponse>(stripped) {
            return resp;
        }
    }

    // 3. Brace-matching: find outermost { ... }
    if let Some(json_str) = extract_json_object(trimmed) {
        if let Ok(resp) = serde_json::from_str::<ChatResponse>(json_str) {
            return resp;
        }
    }

    // 4. Fallback — include parse error hint for debugging
    ChatResponse {
        action: Vec::new(),
        message: raw.to_string(),
    }
}

/// Extract the outermost JSON object from text using brace-matching.
/// Returns the slice `{ ... }` if found, or `None`.
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in text[start..].char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

// ── Video context ───────────────────────────────────────────────────────────

pub struct VideoContext {
    pub video: MediaInfo,
    pub transcript: Option<Transcript>,
    pub current_position: Duration,
    pub chat_history: Vec<ChatMessage>,
    /// Current search results visible in the TUI, injected into the
    /// system prompt so the AI can reference them by index or title.
    pub search_results: Vec<String>,
}

impl VideoContext {
    pub fn new(video: MediaInfo, transcript: Option<Transcript>) -> Self {
        Self {
            video,
            transcript,
            current_position: Duration::ZERO,
            chat_history: Vec::new(),
            search_results: Vec::new(),
        }
    }

    /// Build system prompt with video context and player-control instructions.
    fn build_system_prompt(&self) -> String {
        let mut prompt = String::with_capacity(4096);

        prompt.push_str(
            "You are Aux 🎵, an AI music companion. \
             Be conversational, fun, and helpful. Answer in the same language the user uses.\n\n",
        );

        if self.video.id.is_empty() {
            prompt.push_str("No track is currently playing. You can still search for music, play from results, and chat.\n");
        } else {
            prompt.push_str(&format!("Video: {}\n", self.video.title));
            if let Some(ch) = &self.video.channel {
                prompt.push_str(&format!("Channel: {}\n", ch));
            }
            if let Some(dur) = self.video.duration {
                prompt.push_str(&format!(
                    "Duration: {}\n",
                    crate::media::types::format_duration(dur as u64)
                ));
            }
        }

        if let Some(ref transcript) = self.transcript {
            let context_text = transcript.text_around(self.current_position, 300);
            if !context_text.is_empty() {
                prompt.push_str(&format!(
                    "\nVideo transcript (around current position):\n{}\n",
                    truncate_text(&context_text, 3000)
                ));
            }
        } else {
            prompt.push_str(
                "\n⚠️ No transcript available. You only know the video title and channel.\n",
            );
        }

        // ── Player control instructions ─────────────────────────────────
        prompt.push_str(
            r#"
## Player Control

You can control the music player. If the user's message is a player command (in ANY language), you MUST respond with ONLY valid JSON in this exact format:

{"action": {"type": "<action_type>", ...params}, "message": "<conversational reply>"}

For composing multiple actions (e.g. search then play), use an array:

{"action": [{"type": "search", "query": "..."}, {"type": "play_result", "index": 1}], "message": "..."}

If it's a normal question (not a player command), respond with:

{"action": null, "message": "<your answer>"}

Available actions:

### Playback
- {"type": "set_volume", "value": 0-100}
- {"type": "mute"}
- {"type": "pause"}
- {"type": "resume"}
- {"type": "seek", "seconds": <float, negative = rewind>}
- {"type": "next"} — play next in search results or queue
- {"type": "prev"} — play previous in search results
- {"type": "set_speed", "value": 0.25-4.0}
- {"type": "set_repeat", "mode": "off"|"one"|"all"}
- {"type": "toggle_shuffle"}
- {"type": "set_sleep", "minutes": <uint>}
- {"type": "cancel_sleep"}

### Search & Play
- {"type": "search", "query": "<search terms>"} — search for music (default source)
- {"type": "search", "query": "<search terms>", "source": "soundcloud"} — search on a specific source (youtube, soundcloud, ytmusic)
- {"type": "play_result", "index": <1-based>} — play item from search results
- {"type": "play_random"} — play random item from search results

### Library
- {"type": "add_favorite"} — add current track to favorites
- {"type": "remove_favorite"} — remove current track from favorites
- {"type": "add_to_queue"} — add current track to queue
- {"type": "clear_queue"} — clear the queue

### Playlist
- {"type": "create_playlist", "name": "<name>"} — create a new playlist
- {"type": "delete_playlist", "name": "<name>"} — delete a playlist
- {"type": "add_to_playlist", "playlist": "<name>"} — add current track to a playlist
- {"type": "play_playlist", "name": "<name>"} — load playlist into queue and play

### Navigation
- {"type": "show_panel", "panel": "<name>"} — show a panel (queue, favorites, history, lyrics, search, chat, playlists)

Examples:
- "find Shape of You" → {"action":{"type":"search","query":"Shape of You Ed Sheeran"},"message":"Searching for you! 🔍"}
- "find lofi on SoundCloud" → {"action":{"type":"search","query":"lofi beats","source":"soundcloud"},"message":"Searching SoundCloud! ☁️"}
- "search Adele on YT Music" → {"action":{"type":"search","query":"Adele","source":"ytmusic"},"message":"Searching YT Music! ♫"}
- "play Shape of You" → {"action":[{"type":"search","query":"Shape of You Ed Sheeran"},{"type":"play_result","index":1}],"message":"Searching and playing! 🎵"}
- "play random sad music" → {"action":[{"type":"search","query":"sad songs"},{"type":"play_random"}],"message":"Playing a random sad track! 🎲"}
- "play the 2nd one" → {"action":{"type":"play_result","index":2},"message":"Playing track #2! ▶️"}
- "next track" → {"action":{"type":"next"},"message":"Skipping to the next one! ⏭"}
- "add to favorites" → {"action":{"type":"add_favorite"},"message":"Added to favorites! ❤️"}
- "show queue" → {"action":{"type":"show_panel","panel":"queue"},"message":"Here's your queue! 📋"}
- "create playlist Chill" → {"action":{"type":"create_playlist","name":"Chill"},"message":"Created playlist Chill! 🎶"}
- "add to playlist Chill" → {"action":{"type":"add_to_playlist","playlist":"Chill"},"message":"Added to Chill! ➕"}
- "play playlist Chill" → {"action":{"type":"play_playlist","name":"Chill"},"message":"Playing playlist Chill! 🎵"}
- "pause" → {"action":{"type":"pause"},"message":"Paused! ⏸️"}
- "turn up the volume" → {"action":{"type":"set_volume","value":85},"message":"Volume set to 85%! 🔊"}

IMPORTANT: Your response must ALWAYS be valid JSON. Never add text outside the JSON object.
"#,
        );

        // ── Inject current search results ────────────────────────────────
        if !self.search_results.is_empty() {
            prompt.push_str("\n## Current Search Results\n");
            prompt.push_str("Use play_result with the index to play one of these:\n");
            for (i, item) in self.search_results.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", i + 1, item));
            }
        }

        prompt
    }

    /// Build messages array for API call
    fn build_messages(&self, user_message: &str) -> Vec<serde_json::Value> {
        let mut messages = Vec::new();

        messages.push(serde_json::json!({
            "role": "system",
            "content": self.build_system_prompt()
        }));

        // Add chat history (last 10 messages to stay within context)
        let history_start = self.chat_history.len().saturating_sub(10);
        for msg in &self.chat_history[history_start..] {
            messages.push(serde_json::json!({
                "role": msg.role,
                "content": msg.content
            }));
        }

        messages.push(serde_json::json!({
            "role": "user",
            "content": user_message
        }));

        messages
    }
}

/// Send a chat message and get AI response with optional player action.
pub async fn chat(
    context: &mut VideoContext,
    user_message: &str,
    resolved: &ResolvedAiConfig,
) -> Result<ChatResponse> {
    let api_key = resolved.api_key.as_deref().unwrap_or("");

    if api_key.is_empty() && resolved.provider != "ollama" {
        bail!("API key not found. Run: aux config ai --setup");
    }

    // Add user message to history
    context.chat_history.push(ChatMessage {
        role: "user".to_string(),
        content: user_message.to_string(),
    });

    let messages = context.build_messages(user_message);

    let (api_url, body) = match resolved.provider.as_str() {
        "anthropic" => {
            let url = format!("{}/v1/messages", resolved.base_url);
            let system_msg = messages
                .first()
                .and_then(|m| m["content"].as_str())
                .unwrap_or("")
                .to_string();
            let user_messages: Vec<_> = messages[1..]
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "role": m["role"],
                        "content": m["content"]
                    })
                })
                .collect();
            let body = serde_json::json!({
                "model": &resolved.model,
                "system": system_msg,
                "messages": user_messages,
                "max_tokens": 1000
            });
            (url, body)
        }
        "gemini" => {
            let url = format!(
                "{}/v1beta/models/{}:generateContent?key={}",
                resolved.base_url, resolved.model, api_key
            );
            let contents: Vec<_> = messages
                .iter()
                .map(|m| {
                    let role = match m["role"].as_str().unwrap_or("user") {
                        "system" | "user" => "user",
                        "assistant" => "model",
                        _ => "user",
                    };
                    serde_json::json!({
                        "role": role,
                        "parts": [{"text": m["content"]}]
                    })
                })
                .collect();
            let body = serde_json::json!({ "contents": contents });
            (url, body)
        }
        "ollama" => {
            let url = format!("{}/api/chat", resolved.base_url);
            let body = serde_json::json!({
                "model": &resolved.model,
                "messages": messages,
                "stream": false
            });
            (url, body)
        }
        _ => {
            // OpenAI-compatible (openai + any unknown provider)
            let url = format!("{}/chat/completions", resolved.base_url);
            let body = serde_json::json!({
                "model": &resolved.model,
                "messages": messages,
                "max_tokens": 1000,
                "temperature": 0.7
            });
            (url, body)
        }
    };

    let client = reqwest::Client::new();
    let mut request = client.post(&api_url).json(&body);

    // Set auth headers
    match resolved.provider.as_str() {
        "anthropic" => {
            request = request
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01");
        }
        "gemini" => {
            // Key is in URL
        }
        "ollama" => {
            // No auth
        }
        _ => {
            // OpenAI-compatible: Bearer token
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
    }

    let response = request.send().await.context("Failed to call AI API")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        bail!("AI API error ({}): {}", status, error_body);
    }

    let response_json: serde_json::Value = response.json().await?;

    // Extract response text
    let ai_text = match resolved.provider.as_str() {
        "anthropic" => response_json["content"][0]["text"]
            .as_str()
            .unwrap_or("(no response)")
            .to_string(),
        "gemini" => response_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("(no response)")
            .to_string(),
        "ollama" => response_json["message"]["content"]
            .as_str()
            .unwrap_or("(no response)")
            .to_string(),
        _ => {
            // OpenAI-compatible
            response_json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("(no response)")
                .to_string()
        }
    };

    // Parse structured response (with graceful fallback)
    let chat_response = parse_chat_response(&ai_text);

    // Add assistant response to history (store the message, not raw JSON)
    context.chat_history.push(ChatMessage {
        role: "assistant".to_string(),
        content: chat_response.message.clone(),
    });

    Ok(chat_response)
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!("{}... [truncated]", &text[..max_chars])
    }
}
