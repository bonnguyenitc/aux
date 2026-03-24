# duet 🎵

A CLI YouTube player with AI companion — written in Rust.

Search, play, and discuss YouTube content right from your terminal. Features a full TUI mode, AI chat companion, play history, favorites, and queue management.

## Requirements

- [Rust](https://rustup.rs/): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- [yt-dlp](https://github.com/yt-dlp/yt-dlp): `brew install yt-dlp`
- [mpv](https://mpv.io/): `brew install mpv`

## Install

```bash
cargo install --path .
```

## Usage

### TUI Mode (default)

```bash
duet         # Launch full-screen TUI
```

### CLI Mode

```bash
# Search & play
duet search "lofi coding music"
duet play <url>

# Playback controls
duet pause
duet resume
duet stop
duet volume 80
duet now

# Library
duet history              # Show play history
duet history --today      # Today only
duet favorites            # List favorites (alias: duet fav)
duet fav add <url>        # Add to favorites
duet queue                # Show queue (alias: duet q)
duet q add <url>          # Add to queue
duet q next               # Play next in queue
duet q clear              # Clear queue
```

### Interactive Controls (while playing)

| Key | Action |
|-----|--------|
| `space` | Pause / Resume |
| `←` / `→` | Seek ±10 seconds |
| `↑` / `↓` | Volume ±5 |
| `c` | Chat with AI about the video |
| `f` | Toggle favorite ❤️ |
| `a` | Add to queue 📋 |
| `s` | New search |
| `q` | Quit |

### TUI Controls

| Key | Action |
|-----|--------|
| Type | Search input |
| `Enter` | Search / Select |
| `j` / `k` or `↑` / `↓` | Navigate results |
| `/` | New search |
| `?` | Help |
| `Esc` | Back / Quit |

### AI Companion 🤖

Chat with AI about the video you're watching. Transcripts are automatically fetched for context.

```bash
# While playing, press [c] to enter chat mode
💬 Chat mode (playing: How to Learn Rust)
You: tóm tắt video này đi
🤖: Video này là tutorial về Rust programming...
You: /quit
```

## Configuration

### Player

```bash
duet config player                       # Show current player settings
duet config player set --volume 80       # Set default volume (0-100)
duet config player set --search-results 10  # Number of search results
duet config player set --backend mpv     # Player backend
```

### YouTube

```bash
duet config youtube                      # Show current YouTube settings
duet config youtube set --format m4a     # Preferred audio format (m4a, opus, webm)
duet config youtube set --backend yt-dlp # YouTube backend
```

### AI Configuration

#### Interactive Setup (recommended)

```bash
duet config ai --setup        # Guided wizard — pick provider, model, paste API key
```

#### One-liner Setup

```bash
# Set default AI provider
duet config ai set --provider openai --model gpt-4.1-mini --api-key-env OPENAI_API_KEY

# Or paste your key directly
duet config ai set --provider openai --model gpt-4.1-mini --api-key sk-xxx
```

#### Profiles

Switch between AI configs without editing files:

```bash
# Add profiles
duet config ai add-profile deep --provider anthropic --model claude-sonnet-4-6 --api-key-env ANTHROPIC_API_KEY
duet config ai add-profile local --provider ollama --model llama4 --base-url http://localhost:11434

# Use profiles
duet chat "summarize" --profile deep
duet chat "summarize" --profile local

# Manage profiles
duet config ai list-profiles
duet config ai remove-profile deep
duet config ai test --profile local
```

#### Custom Providers (OpenAI-compatible)

Any OpenAI-compatible API works with `--base-url`:

```bash
duet config ai add-profile groq \
  --provider openai \
  --model llama-3.3-70b \
  --base-url https://api.groq.com/openai/v1 \
  --api-key-env GROQ_API_KEY
```

#### Config File

`~/.config/duet/config.toml`:

```toml
[ai]
provider = "openai"
model = "gpt-4.1-mini"
api_key_env = "OPENAI_API_KEY"

[ai.profiles.deep]
provider = "anthropic"
model = "claude-sonnet-4-6"
api_key_env = "ANTHROPIC_API_KEY"

[ai.profiles.local]
provider = "ollama"
model = "llama4"
base_url = "http://localhost:11434"
```

#### CLI Reference

```bash
duet config ai                  # Show current config + profiles
duet config ai --setup          # Interactive wizard
duet config ai set [FLAGS]      # Set default config
duet config ai add-profile NAME [FLAGS]  # Create/update profile
duet config ai remove-profile NAME       # Delete profile
duet config ai list-profiles    # List all profiles
duet config ai test             # Test default connection
duet config ai test --profile X # Test specific profile
```

## Architecture

```
src/
├── main.rs          # Entry point, CLI + TUI dispatch
├── cli.rs           # clap command definitions
├── config.rs        # TOML config (~/.config/duet/)
├── error.rs         # Custom error types
├── interactive.rs   # Keyboard-driven playback controls
├── ai/
│   ├── chat.rs      # AI chat (OpenAI, Anthropic, Gemini, Ollama)
│   └── transcript.rs # VTT subtitle parsing
├── library/
│   ├── db.rs        # SQLite database
│   ├── history.rs   # Play history
│   ├── favorites.rs # Bookmarks
│   └── queue.rs     # Play queue
├── player/
│   └── mpv.rs       # mpv IPC playback
├── tui/
│   ├── app.rs       # TUI state
│   └── ui.rs        # ratatui rendering
└── youtube/
    ├── ytdlp.rs     # yt-dlp search & stream
    └── types.rs     # VideoInfo, StreamUrl
```

## Roadmap

- [x] Phase 1: Core Player (search, play, controls)
- [x] Phase 2: AI Companion (chat, transcript, multi-provider)
- [x] Phase 3: Queue & Library (history, favorites, queue with SQLite)
- [x] Phase 4: TUI (full-screen ratatui interface)

## License

MIT
