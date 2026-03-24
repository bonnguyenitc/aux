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

### AI Configuration

Add to `~/.config/duet/config.toml`:

```toml
[ai]
provider = "openai"           # "openai" | "anthropic" | "gemini" | "ollama"
model = "gpt-4o-mini"
api_key_env = "OPENAI_API_KEY"

# For local AI (no API key needed):
# [ai]
# provider = "ollama"
# model = "llama3"
```

Then set your API key:

```bash
export OPENAI_API_KEY=your-key-here
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
