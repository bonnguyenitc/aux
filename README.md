# duet 🎵

A CLI YouTube player with AI companion — written in Rust.

Search, play, and discuss YouTube content right from your terminal. Features a full TUI mode with 9 panels, AI chat companion, auto-synced lyrics, playlists, equalizer, sleep timer, and queue management.

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
duet         # Launch full-screen TUI with 9 tabs
```

**Panels:** Search → Results → Lyrics → Queue → Favorites → History → Playlists → Chat → Help

Press `Tab` to cycle between panels, `?` for built-in help.

### CLI Mode

```bash
# Search & play
duet search "lofi coding music"
duet play <url>
duet play <url> --daemon        # Background playback
duet play <url> --speed 1.5     # Start at 1.5x speed

# Playback controls
duet pause
duet resume
duet stop
duet now                        # Show what's playing
duet volume 80
duet seek +30                   # Seek forward 30s
duet seek -10                   # Seek back 10s
duet seek 2:30                  # Seek to 2:30
duet next                       # Skip to next in queue
duet prev                       # Go to previous / restart
duet speed 1.5                  # Set speed
duet speed up                   # Next speed preset
duet repeat one                 # Set repeat (off/one/all)
duet shuffle                    # Toggle shuffle
duet sleep 30m                  # Sleep timer (30m, 1h, 2h, off)

# Library
duet history                    # Show play history
duet history --today            # Today only
duet favorites                  # List favorites (alias: duet fav)
duet fav add <url>              # Add to favorites
duet fav remove <video_id>      # Remove favorite

# Queue
duet queue                      # Show queue (alias: duet q)
duet q add <url>                # Add to queue
duet q next                     # Play next
duet q clear                    # Clear queue

# Playlists
duet playlist list              # List playlists (alias: duet pl)
duet pl create "Chill Vibes"    # Create playlist
duet pl show "Chill Vibes"      # Show items
duet pl add "Chill Vibes" <url> # Add track
duet pl remove "Chill Vibes" <id> # Remove track
duet pl play "Chill Vibes"      # Play all (loads to queue)
duet pl delete "Chill Vibes"    # Delete playlist

# Equalizer
duet eq                         # Show current preset
duet eq bass-boost              # Set preset (flat/bass-boost/vocal/treble/loudness)

# AI
duet chat "summarize this"      # Chat about current video
duet chat --profile deep        # Use specific AI profile
duet suggest                    # AI-powered video suggestions
```

### TUI Keybindings

#### Navigation

| Key | Action |
|-----|--------|
| `Tab` | Cycle panels |
| `/` | New search |
| `Enter` | Play / confirm / select |
| `↑` `↓` / `j` `k` | Navigate list items |
| `←` `→` | Page prev / next (Results) |
| `Esc` | Back / close sub-view |
| `q` | Back / quit (not in Chat/Search) |
| `?` | Help panel |

#### Playback (any panel while playing)

| Key | Action |
|-----|--------|
| `Space` | Pause / Resume |
| `←` `→` | Seek ±10s (non-list panels) |
| `Shift+←` `→` | Seek ±60s |
| `+` / `-` (or `=`) | Volume ±5% |
| `]` / `[` | Speed up / down (0.25x–4.0x) |
| `n` | Skip to next track |
| `p` | Restart / previous track |
| `r` | Cycle repeat (off → one → all) |
| `z` | Toggle shuffle 🔀 |
| `f` | Toggle favorite ❤️ |
| `a` | Add/remove current track to queue 📋 |
| `S` | Stop playback |
| `t` | Sleep timer (15m → 30m → 1h → 2h → off) 😴 |
| `e` | Cycle equalizer preset 🎛️ |

#### Lyrics Panel

| Key | Action |
|-----|--------|
| `Shift+↑` `↓` | Scroll lyrics manually |
| `0` | Reset auto-scroll |

#### Playlists Panel

| Key | Action |
|-----|--------|
| `Enter` | View playlist items / Play selected item |
| `n` | Create new playlist (inline input) |
| `d` | Delete selected playlist |
| `p` | Play entire playlist (loads to queue) |
| `Esc` | Back to list / back to Search |

#### Chat Panel

| Key | Action |
|-----|--------|
| Type | Text input |
| `Enter` | Send message to AI |
| `↑` / `↓` | Scroll chat history |
| `Esc` | Back to Search |

### AI Companion 🤖

Chat with AI about the video you're watching. Transcripts are automatically fetched for context.

```bash
# CLI: one-shot or interactive
duet chat "summarize this video"
duet chat                         # Interactive mode

# TUI: Tab to Chat panel, type your message
```

## Equalizer Presets 🎛️

Five built-in presets using mpv's superequalizer:

| Preset | Description |
|--------|-------------|
| `flat` | No EQ (default) |
| `bass-boost` | Enhanced bass frequencies |
| `vocal` | Boosted mid-range for vocals |
| `treble` | Enhanced high frequencies |
| `loudness` | Full-range boost |

Press `e` in TUI or use `duet eq <preset>` in CLI. The active preset persists across sessions.

## Configuration

### Player

```bash
duet config player                       # Show current settings
duet config player set --volume 80       # Default volume (0-100)
duet config player set --search-results 10  # Results per search
```

### YouTube

```bash
duet config youtube                      # Show current settings
duet config youtube set --format m4a     # Audio format (m4a, opus, webm)
```

### AI Configuration

#### Interactive Setup (recommended)

```bash
duet config ai --setup        # Guided wizard — pick provider, model, paste API key
```

#### One-liner Setup

```bash
duet config ai set --provider openai --model gpt-4.1-mini --api-key-env OPENAI_API_KEY
duet config ai set --provider openai --model gpt-4.1-mini --api-key sk-xxx
```

#### Profiles

```bash
# Add profiles
duet config ai add-profile deep --provider anthropic --model claude-sonnet-4-6 --api-key-env ANTHROPIC_API_KEY
duet config ai add-profile local --provider ollama --model llama4 --base-url http://localhost:11434

# Use profiles
duet chat "summarize" --profile deep
duet suggest --profile local

# Manage profiles
duet config ai list-profiles
duet config ai remove-profile deep
duet config ai test --profile local
```

#### Custom Providers (OpenAI-compatible)

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

## Architecture

```
src/
├── main.rs          # Entry point, CLI + TUI dispatch
├── cli.rs           # clap command definitions
├── config.rs        # TOML config (~/.config/duet/)
├── error.rs         # Custom error types
├── interactive.rs   # Keyboard-driven playback controls (CLI mode)
├── util.rs          # Utilities (duration parsing, speed presets)
├── ai/
│   ├── chat.rs      # AI chat (OpenAI, Anthropic, Gemini, Ollama)
│   └── transcript.rs # VTT subtitle parsing & auto-sync lyrics
├── library/
│   ├── db.rs        # SQLite database
│   ├── history.rs   # Play history
│   ├── favorites.rs # Bookmarks
│   ├── queue.rs     # Play queue
│   ├── playlist.rs  # Playlist management (CRUD + load-to-queue)
│   └── search_history.rs # Search query recall
├── player/
│   ├── mod.rs       # MediaPlayer trait
│   ├── mpv.rs       # mpv IPC playback (retry, timeout, audio filters)
│   ├── state.rs     # State file (speed, repeat, shuffle, EQ, sleep)
│   └── queue_manager.rs # Queue advancement logic
├── tui/
│   ├── app.rs       # TUI state (9 panels + now-playing)
│   └── ui.rs        # ratatui rendering (tabs, tables, lyrics, chat)
└── youtube/
    ├── ytdlp.rs     # yt-dlp search & stream
    └── types.rs     # VideoInfo, StreamUrl
```

## Feature Comparison with Spotify

| Feature | Spotify | Duet |
|---------|---------|------|
| Search & play | ✅ | ✅ |
| Queue management | ✅ | ✅ |
| Playlists (create/edit/play) | ✅ | ✅ |
| Favorites | ✅ | ✅ |
| Play history | ✅ | ✅ |
| Repeat (off/one/all) | ✅ | ✅ |
| Shuffle | ✅ | ✅ |
| Speed control (0.25x–4x) | ✅ | ✅ |
| Sleep timer | ✅ | ✅ |
| Equalizer presets | ✅ | ✅ |
| Lyrics (auto-synced) | ✅ | ✅ |
| AI chat companion | ❌ | ✅ |
| AI suggestions | ❌ | ✅ |
| Terminal / keyboard-only | ❌ | ✅ |
| Open source | ❌ | ✅ |
| Crossfade | ✅ | ❌ |
| Multi-device | ✅ | ❌ |
| Offline download | ✅ | ❌ |

## Roadmap

- [x] Phase 1: Core Player (search, play, controls)
- [x] Phase 2: AI Companion (chat, transcript, multi-provider)
- [x] Phase 3: Queue & Library (history, favorites, queue with SQLite)
- [x] Phase 4: TUI (full-screen ratatui interface with 9 panels)
- [x] Phase 5: Playlists & Equalizer (Spotify feature parity)

## License

MIT
