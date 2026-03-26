use anyhow::{bail, Context, Result};
use colored::*;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use std::collections::HashMap;

use crate::config::{default_base_url, AiConfig, AiProfile, Config, ResolvedAiConfig};

// ─── Model Suggestions (wizard only) ──────────────────────────

pub struct ProviderSuggestion {
    pub id: &'static str,
    pub label: &'static str,
    pub default_api_key_env: &'static str,
    pub models: &'static [(&'static str, &'static str)],
}

pub const PROVIDER_SUGGESTIONS: &[ProviderSuggestion] = &[
    ProviderSuggestion {
        id: "openai",
        label: "OpenAI",
        default_api_key_env: "OPENAI_API_KEY",
        models: &[
            ("gpt-4.1-nano", "cheapest, fastest"),
            ("gpt-4.1-mini", "⭐ recommend — fast + cheap"),
            ("gpt-4.1", "flagship"),
            ("gpt-5.4", "⭐ latest flagship"),
            ("o4-mini", "reasoning, fast"),
        ],
    },
    ProviderSuggestion {
        id: "anthropic",
        label: "Anthropic",
        default_api_key_env: "ANTHROPIC_API_KEY",
        models: &[
            ("claude-haiku-4-5", "⭐ recommend — fast, cheap"),
            ("claude-sonnet-4-6", "⭐ latest balanced"),
            ("claude-opus-4-6", "flagship, best"),
        ],
    },
    ProviderSuggestion {
        id: "gemini",
        label: "Google Gemini",
        default_api_key_env: "GEMINI_API_KEY",
        models: &[
            ("gemini-2.5-flash", "⭐ recommend"),
            ("gemini-2.5-pro", "best"),
            ("gemini-3.1-pro-preview", "latest preview"),
        ],
    },
    ProviderSuggestion {
        id: "ollama",
        label: "Ollama (local, no API key)",
        default_api_key_env: "",
        models: &[
            ("llama4", ""),
            ("llama3.3", ""),
            ("gemma3", ""),
            ("deepseek-r1", "reasoning"),
            ("qwen3", ""),
        ],
    },
];

const CUSTOM_MODEL_LABEL: &str = "✏️  Enter custom model name...";
const OTHER_PROVIDER_LABEL: &str = "Other (OpenAI-compatible)";

// ─── Display ──────────────────────────────────────────────────

pub fn show_all(config: &Config) {
    println!(
        "\n  {} {}\n",
        "⚙️  Config".bold(),
        Config::config_path()
            .map(|p| format!("({})", p.display()))
            .unwrap_or_default()
            .dimmed()
    );

    println!("  {}", "[player]".cyan().bold());
    println!("  default_volume  = {}", config.player.default_volume);
    println!("  search_results  = {}", config.player.search_results);
    println!();

    println!("  {}", "[media]".cyan().bold());
    println!("  prefer_format   = {}", config.media.prefer_format);
    println!("  default_source  = {}", config.media.default_source);
    println!();

    println!("  {}", "[ai]".cyan().bold());
    if let Some(ref ai) = config.ai {
        show_ai_detail(ai);
    } else {
        println!(
            "  {}",
            "(not configured — run: aux config ai --setup)".dimmed()
        );
    }
    println!();
}

pub fn show_ai(config: &Config) {
    if let Some(ref ai) = config.ai {
        println!("\n  {}", "🤖 AI Config".bold().cyan());
        show_ai_detail(ai);
    } else {
        println!(
            "\n  {}",
            "AI not configured. Run: aux config ai --setup".yellow()
        );
    }
    println!();
}

fn show_ai_detail(ai: &AiConfig) {
    let resolved_url = ai
        .base_url
        .as_deref()
        .filter(|u| !u.is_empty())
        .map(|u| u.to_string())
        .unwrap_or_else(|| format!("{} (default)", default_base_url(&ai.provider)));

    println!("  provider    = {}", ai.provider.yellow());
    println!("  model       = {}", ai.model.yellow());
    println!("  base_url    = {}", resolved_url.dimmed());
    println!(
        "  api_key     = {}",
        format_key_status(ai.api_key.as_deref(), ai.api_key_env.as_deref())
    );

    if !ai.profiles.is_empty() {
        println!();
        println!("  {}", "Profiles:".cyan().bold());
        for (name, profile) in &ai.profiles {
            let provider = profile.provider.as_deref().unwrap_or(&ai.provider);
            let model = profile.model.as_deref().unwrap_or(&ai.model);
            let key_status = format_key_status(
                profile.api_key.as_deref(),
                profile.api_key_env.as_deref().or(ai.api_key_env.as_deref()),
            );
            println!(
                "    {:<12}{} / {:<28}{}",
                name.bold(),
                provider,
                model,
                key_status
            );
        }
    }
}

fn format_key_status(api_key: Option<&str>, api_key_env: Option<&str>) -> String {
    if let Some(k) = api_key {
        if !k.is_empty() {
            return format!("✅ set ({}...)", &k[..k.len().min(6)])
                .green()
                .to_string();
        }
    }
    if let Some(env_var) = api_key_env {
        if env_var.is_empty() {
            return "✅ no key needed".green().to_string();
        }
        match std::env::var(env_var) {
            Ok(v) if !v.is_empty() => {
                return format!("✅ via ${} ({}...)", env_var, &v[..v.len().min(6)])
                    .green()
                    .to_string();
            }
            _ => {}
        }
    }
    "⚠️  not set".yellow().to_string()
}

pub fn show_path() {
    match Config::config_path() {
        Some(p) => println!("  {}", p.display()),
        None => println!("  {}", "Could not determine config path".red()),
    }
}

// ─── Player Commands ──────────────────────────────────────────

pub fn show_player(config: &Config) {
    println!("\n  {}", "🎵 Player Config".bold().cyan());
    println!(
        "  volume          = {}",
        config.player.default_volume.to_string().yellow()
    );
    println!(
        "  search_results  = {}",
        config.player.search_results.to_string().yellow()
    );
    println!("  backend         = {}", config.player.backend.yellow());
    println!();
}

pub fn player_set(
    config: &mut Config,
    volume: Option<u8>,
    search_results: Option<usize>,
    backend: Option<String>,
) -> Result<()> {
    let mut changed = Vec::new();

    if let Some(v) = volume {
        if v > 100 {
            bail!("Volume must be between 0 and 100");
        }
        config.player.default_volume = v;
        changed.push(format!("volume = {}", v));
    }
    if let Some(s) = search_results {
        if s == 0 {
            bail!("search_results must be >= 1");
        }
        config.player.search_results = s;
        changed.push(format!("search_results = {}", s));
    }
    if let Some(b) = backend {
        config.player.backend = b.clone();
        changed.push(format!("backend = {}", b));
    }

    if changed.is_empty() {
        bail!("No flags provided. Use --volume, --search-results, or --backend");
    }

    config.save()?;
    for c in &changed {
        println!("  {} {}", "✅ Set".green(), c.cyan());
    }
    Ok(())
}

// ─── Media Commands ───────────────────────────────────────────

pub fn show_media(config: &Config) {
    println!("\n  {}", "🎵 Media Config".bold().cyan());
    println!(
        "  prefer_format   = {}",
        config.media.prefer_format.yellow()
    );
    println!("  backend         = {}", config.media.backend.yellow());
    println!(
        "  default_source  = {}",
        config.media.default_source.yellow()
    );
    println!();
}

pub fn media_set(
    config: &mut Config,
    format: Option<String>,
    backend: Option<String>,
    default_source: Option<String>,
) -> Result<()> {
    let mut changed = Vec::new();

    if let Some(f) = format {
        config.media.prefer_format = f.clone();
        changed.push(format!("prefer_format = {}", f));
    }
    if let Some(b) = backend {
        config.media.backend = b.clone();
        changed.push(format!("backend = {}", b));
    }
    if let Some(s) = default_source {
        config.media.default_source = s.clone();
        changed.push(format!("default_source = {}", s));
    }

    if changed.is_empty() {
        bail!("No flags provided. Use --format, --backend, or --default-source");
    }

    config.save()?;
    for c in &changed {
        println!("  {} {}", "✅ Set".green(), c.cyan());
    }
    Ok(())
}

// ─── AI Set Command ───────────────────────────────────────────

pub fn ai_set(
    config: &mut Config,
    provider: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let ai = config.ai.get_or_insert_with(default_ai);

    let mut changed = Vec::new();

    if let Some(p) = provider {
        ai.provider = p.clone();
        changed.push(format!("provider = {}", p));
    }
    if let Some(m) = model {
        ai.model = m.clone();
        changed.push(format!("model = {}", m));
    }
    if let Some(k) = api_key {
        ai.api_key = Some(k);
        changed.push("api_key = ***".to_string());
    }
    if let Some(e) = api_key_env {
        ai.api_key_env = Some(e.clone());
        changed.push(format!("api_key_env = {}", e));
    }
    if let Some(u) = base_url {
        ai.base_url = Some(u.clone());
        changed.push(format!("base_url = {}", u));
    }

    if changed.is_empty() {
        bail!(
            "No flags provided. Use --provider, --model, --api-key, --api-key-env, or --base-url"
        );
    }

    config.save()?;
    for c in &changed {
        println!("  {} {}", "✅ Set".green(), c.cyan());
    }
    Ok(())
}

// ─── Profile Management ──────────────────────────────────────

pub fn add_profile(
    config: &mut Config,
    name: &str,
    provider: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let ai = config.ai.get_or_insert_with(default_ai);

    let profile = ai
        .profiles
        .entry(name.to_string())
        .or_insert_with(|| AiProfile {
            provider: None,
            model: None,
            api_key: None,
            api_key_env: None,
            base_url: None,
        });

    if let Some(p) = provider {
        profile.provider = Some(p);
    }
    if let Some(m) = model {
        profile.model = Some(m);
    }
    if let Some(k) = api_key {
        profile.api_key = Some(k);
    }
    if let Some(e) = api_key_env {
        profile.api_key_env = Some(e);
    }
    if let Some(u) = base_url {
        profile.base_url = Some(u);
    }

    config.save()?;
    println!("  {} profile '{}' saved", "✅".green(), name.cyan());
    Ok(())
}

pub fn remove_profile(config: &mut Config, name: &str) -> Result<()> {
    let ai = config.ai.as_mut().context("AI not configured")?;
    if ai.profiles.remove(name).is_none() {
        bail!("Profile '{}' not found", name);
    }
    config.save()?;
    println!("  {} profile '{}' removed", "🗑️", name);
    Ok(())
}

pub fn list_profiles(config: &Config) {
    match config.ai.as_ref() {
        Some(ai) if !ai.profiles.is_empty() => {
            println!("\n  {}", "AI Profiles".bold().cyan());
            for (name, profile) in &ai.profiles {
                let provider = profile.provider.as_deref().unwrap_or(&ai.provider);
                let model = profile.model.as_deref().unwrap_or(&ai.model);
                let key_status = format_key_status(
                    profile.api_key.as_deref(),
                    profile.api_key_env.as_deref().or(ai.api_key_env.as_deref()),
                );
                println!(
                    "    {:<12}{} / {:<28}{}",
                    name.bold(),
                    provider,
                    model,
                    key_status
                );
            }
            println!();
        }
        _ => {
            println!("  {}", "No profiles configured.".dimmed());
        }
    }
}

// ─── Key/Value Management ─────────────────────────────────────

pub fn set_key(config: &mut Config, key: &str, value: &str) -> Result<()> {
    match key {
        "ai.provider" => {
            config.ai.get_or_insert_with(default_ai).provider = value.to_string();
        }
        "ai.model" => {
            config.ai.get_or_insert_with(default_ai).model = value.to_string();
        }
        "ai.api_key_env" => {
            config.ai.get_or_insert_with(default_ai).api_key_env = Some(value.to_string());
        }
        "ai.base_url" => {
            config.ai.get_or_insert_with(default_ai).base_url = Some(value.to_string());
        }
        "player.default_volume" => {
            let v: u8 = value
                .parse()
                .context("default_volume must be a number between 0 and 100")?;
            if v > 100 {
                bail!("default_volume must be between 0 and 100");
            }
            config.player.default_volume = v;
        }
        "player.search_results" => {
            let v: usize = value
                .parse()
                .context("search_results must be a positive number")?;
            if v == 0 {
                bail!("search_results must be >= 1");
            }
            config.player.search_results = v;
        }
        "media.prefer_format" | "youtube.prefer_format" => {
            config.media.prefer_format = value.to_string();
        }
        "media.default_source" => {
            config.media.default_source = value.to_string();
        }
        _ => bail!(
            "Unknown config key: '{}'.\nValid keys: ai.provider, ai.model, ai.api_key_env, ai.base_url, player.default_volume, player.search_results, media.prefer_format, media.default_source",
            key
        ),
    }
    config.save()?;
    println!("  {} {} = {}", "✅ Set".green(), key.cyan(), value.yellow());
    Ok(())
}

pub fn get_key(config: &Config, key: &str) -> Result<String> {
    let value = match key {
        "ai.provider" => config.ai.as_ref().map(|a| a.provider.clone()),
        "ai.model" => config.ai.as_ref().map(|a| a.model.clone()),
        "ai.api_key_env" => config.ai.as_ref().and_then(|a| a.api_key_env.clone()),
        "ai.base_url" => config.ai.as_ref().and_then(|a| a.base_url.clone()),
        "player.default_volume" => Some(config.player.default_volume.to_string()),
        "player.search_results" => Some(config.player.search_results.to_string()),
        "media.prefer_format" | "youtube.prefer_format" => Some(config.media.prefer_format.clone()),
        "media.default_source" => Some(config.media.default_source.clone()),
        _ => bail!("Unknown config key: '{}'", key),
    };

    match value {
        Some(v) => {
            println!("{}", v);
            Ok(v)
        }
        None => bail!("Key '{}' is not set", key),
    }
}

pub fn reset_config(force: bool) -> Result<()> {
    if !force {
        let ok = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Reset all config to defaults?")
            .default(false)
            .interact()?;
        if !ok {
            println!("  {}", "Cancelled.".dimmed());
            return Ok(());
        }
    }
    Config::default().save()?;
    println!("  {} config reset to defaults", "✅".green());
    Ok(())
}

fn default_ai() -> AiConfig {
    AiConfig {
        provider: "openai".to_string(),
        model: "gpt-4.1-mini".to_string(),
        api_key: None,
        api_key_env: Some("OPENAI_API_KEY".to_string()),
        base_url: None,
        profiles: HashMap::new(),
    }
}

// ─── AI Setup Wizard ──────────────────────────────────────────

pub async fn run_ai_wizard(config: &mut Config) -> Result<()> {
    let theme = ColorfulTheme::default();

    println!("\n  {}\n", "🤖 Aux AI Setup".bold().cyan());

    // Step 1: Choose provider
    let mut provider_labels: Vec<&str> = PROVIDER_SUGGESTIONS.iter().map(|p| p.label).collect();
    provider_labels.push(OTHER_PROVIDER_LABEL);

    let provider_idx = Select::with_theme(&theme)
        .with_prompt("Choose AI provider")
        .items(&provider_labels)
        .default(0)
        .interact()?;

    let is_custom_provider = provider_idx == PROVIDER_SUGGESTIONS.len();

    let (provider_id, provider_label, default_env, model, base_url) = if is_custom_provider {
        // Custom OpenAI-compatible provider
        let id: String = Input::with_theme(&theme)
            .with_prompt("Provider name (e.g. groq, together, fireworks)")
            .interact_text()?;

        let model: String = Input::with_theme(&theme)
            .with_prompt("Model name")
            .interact_text()?;

        let url: String = Input::with_theme(&theme)
            .with_prompt("Base URL (required)")
            .interact_text()?;

        let env: String = Input::with_theme(&theme)
            .with_prompt("API key env var name (e.g. GROQ_API_KEY)")
            .interact_text()?;

        (id.clone(), id, env, model, Some(url))
    } else {
        let suggestion = &PROVIDER_SUGGESTIONS[provider_idx];

        // Step 2: Choose model from suggestions
        let mut model_labels: Vec<String> = suggestion
            .models
            .iter()
            .map(|(name, note)| {
                if note.is_empty() {
                    name.to_string()
                } else {
                    format!("{}  {}", name, note.dimmed())
                }
            })
            .collect();
        model_labels.push(CUSTOM_MODEL_LABEL.to_string());

        let model_idx = Select::with_theme(&theme)
            .with_prompt("Choose model")
            .items(&model_labels)
            .default(0)
            .interact()?;

        let model = if model_idx == suggestion.models.len() {
            Input::with_theme(&theme)
                .with_prompt("Model name")
                .interact_text()?
        } else {
            suggestion.models[model_idx].0.to_string()
        };

        // Step 2b: Base URL (optional for known providers)
        let provider_default_url = default_base_url(suggestion.id);
        let url_input: String = Input::with_theme(&theme)
            .with_prompt(format!(
                "Base URL (Enter for default: {})",
                provider_default_url
            ))
            .default(String::new())
            .allow_empty(true)
            .interact_text()?;

        let base_url = if url_input.is_empty() {
            None
        } else {
            Some(url_input)
        };

        (
            suggestion.id.to_string(),
            suggestion.label.to_string(),
            suggestion.default_api_key_env.to_string(),
            model,
            base_url,
        )
    };

    // Step 3: API key (skip for ollama)
    let (api_key, api_key_env) = if provider_id == "ollama" {
        (None, None)
    } else {
        let existing_key = config
            .ai
            .as_ref()
            .and_then(|a| a.api_key.clone())
            .or_else(|| std::env::var(&default_env).ok().filter(|v| !v.is_empty()));

        let hint = if existing_key.is_some() {
            " (press Enter to keep current key)"
        } else {
            ""
        };

        println!(
            "  {} Paste your {} API key below{}",
            "🔑".dimmed(),
            provider_label.cyan(),
            hint.dimmed()
        );

        let key_input: String = Input::with_theme(&theme)
            .with_prompt("API key")
            .default(existing_key.unwrap_or_default())
            .allow_empty(true)
            .interact_text()?;

        let api_key = if key_input.is_empty() {
            None
        } else {
            let masked = format!(
                "{}...{}",
                &key_input[..key_input.len().min(6)],
                &key_input[key_input.len().saturating_sub(4)..]
            );
            println!("  {} Key: {}", "✅".green(), masked.dimmed());
            Some(key_input)
        };

        let env = if default_env.is_empty() {
            None
        } else {
            Some(default_env)
        };
        (api_key, env)
    };

    // Build config
    let new_ai = AiConfig {
        provider: provider_id.clone(),
        model: model.clone(),
        api_key: api_key.clone(),
        api_key_env: api_key_env.clone(),
        base_url: base_url.clone(),
        profiles: config
            .ai
            .as_ref()
            .map(|a| a.profiles.clone())
            .unwrap_or_default(),
    };

    // Step 4: Test connection
    let resolved = new_ai.resolve(None)?;
    let has_key = resolved.api_key.is_some() || provider_id == "ollama";

    if has_key {
        println!(
            "\n  🔑 Testing connection to {} ({})...",
            provider_label.cyan(),
            model.yellow()
        );

        match test_connection(&resolved).await {
            Ok(_) => {
                println!("  {} Connected! AI companion is ready.", "✅".green());
            }
            Err(e) => {
                println!("  {} Connection failed: {}", "❌".red(), e);
                let save_anyway = Confirm::with_theme(&theme)
                    .with_prompt("Save config anyway?")
                    .default(true)
                    .interact()?;
                if !save_anyway {
                    println!("  {}", "Cancelled. Config not saved.".dimmed());
                    return Ok(());
                }
            }
        }
    } else {
        println!(
            "\n  {} Skipping connection test — no API key provided",
            "⚠️".yellow()
        );
    }

    // Step 5: Save as profile or default?
    let profile_name: String = Input::with_theme(&theme)
        .with_prompt("Save as profile? (leave empty to set as default)")
        .default(String::new())
        .allow_empty(true)
        .interact_text()?;

    if profile_name.is_empty() {
        config.ai = Some(new_ai);
    } else {
        let ai = config.ai.get_or_insert_with(default_ai);
        ai.profiles.insert(
            profile_name.clone(),
            AiProfile {
                provider: Some(new_ai.provider),
                model: Some(new_ai.model),
                api_key: new_ai.api_key,
                api_key_env: new_ai.api_key_env,
                base_url: new_ai.base_url,
            },
        );
        println!(
            "  {} Saved as profile '{}'",
            "✅".green(),
            profile_name.cyan()
        );
    }

    config.save()?;
    println!(
        "  {} Config saved to {}",
        "💾".green(),
        Config::config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
            .dimmed()
    );
    println!();

    Ok(())
}

// ─── Connection Test ──────────────────────────────────────────

async fn test_connection(resolved: &ResolvedAiConfig) -> Result<()> {
    if resolved.api_key.is_none() && resolved.provider != "ollama" {
        bail!("No API key found — set via --api-key or env var");
    }

    let api_key = resolved.api_key.as_deref().unwrap_or("");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let (url, body, auth_header): (String, serde_json::Value, Option<String>) =
        match resolved.provider.as_str() {
            "anthropic" => (
                format!("{}/v1/messages", resolved.base_url),
                serde_json::json!({
                    "model": resolved.model,
                    "messages": [{"role": "user", "content": "hi"}],
                    "max_tokens": 5
                }),
                None,
            ),
            "gemini" => (
                format!(
                    "{}/v1beta/models/{}:generateContent?key={}",
                    resolved.base_url, resolved.model, api_key
                ),
                serde_json::json!({
                    "contents": [{"role": "user", "parts": [{"text": "hi"}]}]
                }),
                None,
            ),
            "ollama" => (
                format!("{}/api/chat", resolved.base_url),
                serde_json::json!({
                    "model": resolved.model,
                    "messages": [{"role": "user", "content": "hi"}],
                    "stream": false
                }),
                None,
            ),
            _ => {
                // OpenAI-compatible (openai + any unknown provider)
                (
                    format!("{}/chat/completions", resolved.base_url),
                    serde_json::json!({
                        "model": resolved.model,
                        "messages": [{"role": "user", "content": "hi"}],
                        "max_tokens": 5
                    }),
                    Some(format!("Bearer {}", api_key)),
                )
            }
        };

    let mut req = client.post(&url).json(&body);

    // Set auth headers
    match resolved.provider.as_str() {
        "anthropic" => {
            req = req
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01");
        }
        "gemini" | "ollama" => {
            // Key in URL for gemini, no key for ollama
        }
        _ => {
            if let Some(ref auth) = auth_header {
                req = req.header("Authorization", auth);
            }
        }
    }

    let resp = req.send().await.context("Request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        bail!(
            "HTTP {} — {}",
            status,
            body_text.chars().take(200).collect::<String>()
        );
    }

    Ok(())
}

// ─── Public test wrapper ──────────────────────────────────────

pub async fn run_test(config: &Config, profile: Option<&str>) -> Result<()> {
    let ai = config
        .ai
        .as_ref()
        .context("AI not configured. Run: aux config ai --setup")?;
    let resolved = ai.resolve(profile)?;

    println!(
        "\n  🔑 Testing {} ({})...",
        resolved.provider.cyan(),
        resolved.model.yellow()
    );

    match test_connection(&resolved).await {
        Ok(_) => {
            println!("  {} Connected!", "✅".green());
        }
        Err(e) => {
            println!("  {} {}", "❌".red(), e);
        }
    }
    println!();
    Ok(())
}
