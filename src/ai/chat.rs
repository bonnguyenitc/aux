use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::ResolvedAiConfig;
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
    resolved: &ResolvedAiConfig,
) -> Result<String> {
    let api_key = resolved.api_key.as_deref().unwrap_or("");

    if api_key.is_empty() && resolved.provider != "ollama" {
        bail!("API key not found. Run: duet config ai --setup");
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
