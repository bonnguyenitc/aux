use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::Config;
use crate::youtube::VideoInfo;
use super::transcript::Transcript;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" or "assistant"
    pub content: String,
}

pub struct VideoContext {
    pub video: VideoInfo,
    pub transcript: Option<Transcript>,
    pub current_position: Duration,
    pub chat_history: Vec<ChatMessage>,
}

impl VideoContext {
    pub fn new(video: VideoInfo, transcript: Option<Transcript>) -> Self {
        Self {
            video,
            transcript,
            current_position: Duration::ZERO,
            chat_history: Vec::new(),
        }
    }

    /// Build system prompt with video context
    fn build_system_prompt(&self) -> String {
        let mut prompt = String::from(
            "You are Duet 🎵, an AI companion watching YouTube with the user. \
             Be conversational, fun, and helpful. Answer in the same language the user uses.\n\n",
        );

        prompt.push_str(&format!("Video: {}\n", self.video.title));
        if let Some(ch) = &self.video.channel {
            prompt.push_str(&format!("Channel: {}\n", ch));
        }
        if let Some(dur) = self.video.duration {
            prompt.push_str(&format!(
                "Duration: {}\n",
                crate::youtube::types::format_duration(dur as u64)
            ));
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

/// Send a chat message and get AI response
pub async fn chat(
    context: &mut VideoContext,
    user_message: &str,
    config: &Config,
) -> Result<String> {
    let ai_config = config
        .ai
        .as_ref()
        .context("AI not configured. Add [ai] section to ~/.config/duet/config.toml")?;

    let api_key = std::env::var(&ai_config.api_key_env).with_context(|| {
        format!(
            "API key not found. Set environment variable: {}",
            ai_config.api_key_env
        )
    })?;

    // Add user message to history
    context.chat_history.push(ChatMessage {
        role: "user".to_string(),
        content: user_message.to_string(),
    });

    let messages = context.build_messages(user_message);

    let (api_url, body) = match ai_config.provider.as_str() {
        "openai" => {
            let url = "https://api.openai.com/v1/chat/completions".to_string();
            let body = serde_json::json!({
                "model": &ai_config.model,
                "messages": messages,
                "max_tokens": 1000,
                "temperature": 0.7
            });
            (url, body)
        }
        "anthropic" => {
            let url = "https://api.anthropic.com/v1/messages".to_string();
            // Convert messages format for Anthropic
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
                "model": &ai_config.model,
                "system": system_msg,
                "messages": user_messages,
                "max_tokens": 1000
            });
            (url, body)
        }
        "gemini" => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                ai_config.model, api_key
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
            let body = serde_json::json!({
                "contents": contents
            });
            (url, body)
        }
        "ollama" => {
            let host = ai_config
                .ollama_host
                .as_deref()
                .unwrap_or("http://localhost:11434");
            let url = format!("{}/api/chat", host);
            let body = serde_json::json!({
                "model": &ai_config.model,
                "messages": messages,
                "stream": false
            });
            (url, body)
        }
        provider => bail!("Unsupported AI provider: {}", provider),
    };

    let client = reqwest::Client::new();
    let mut request = client.post(&api_url).json(&body);

    // Set auth headers based on provider
    match ai_config.provider.as_str() {
        "openai" => {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
        "anthropic" => {
            request = request
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01");
        }
        "gemini" => {
            // Key is in URL for Gemini
        }
        _ => {}
    }

    let response = request.send().await.context("Failed to call AI API")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        bail!("AI API error ({}): {}", status, error_body);
    }

    let response_json: serde_json::Value = response.json().await?;

    // Extract response text based on provider
    let ai_text = match ai_config.provider.as_str() {
        "openai" | "ollama" => response_json["choices"][0]["message"]["content"]
            .as_str()
            .or_else(|| response_json["message"]["content"].as_str())
            .unwrap_or("(no response)")
            .to_string(),
        "anthropic" => response_json["content"][0]["text"]
            .as_str()
            .unwrap_or("(no response)")
            .to_string(),
        "gemini" => response_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("(no response)")
            .to_string(),
        _ => "(unsupported provider)".to_string(),
    };

    // Add assistant response to history
    context.chat_history.push(ChatMessage {
        role: "assistant".to_string(),
        content: ai_text.clone(),
    });

    Ok(ai_text)
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!("{}... [truncated]", &text[..max_chars])
    }
}
