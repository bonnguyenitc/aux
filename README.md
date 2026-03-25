# aux 🎵

Listen music with AI agent — a universal audio player written in Rust.

Search and play audio from YouTube, SoundCloud, YT Music, and 1000+ sites supported by yt-dlp. Features a full TUI mode with 9 panels, AI chat companion, auto-synced lyrics, playlists, equalizer, sleep timer, and queue management.

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
aux         # Launch full-screen TUI with 9 tabs
```

**Panels:** Search → Results → Lyrics → Queue → Favorites → History → Playlists → Chat → Help

Press `Tab` to cycle between panels, `?` for built-in help.

### CLI Mode

```bash
# Search & play
aux search "lofi coding music"                # YouTube (default)
aux search "ambient" --source soundcloud       # SoundCloud
aux search "focus beats" --source ytmusic       # YT Music
aux play <url>                                  # Any supported URL
aux play <url> --daemon        # Background playback
aux play <url> --speed 1.5     # Start at 1.5x speed

# Playback controls
aux pause
aux resume
aux stop
aux now                        # Show what's playing
aux volume 80
aux seek +30                   # Seek forward 30s
aux seek -10                   # Seek back 10s
aux seek 2:30                  # Seek to 2:30
aux next                       # Skip to next in queue
aux prev                       # Go to previous / restart
aux speed 1.5                  # Set speed
aux speed up                   # Next speed preset
aux repeat one                 # Set repeat (off/one/all)
aux shuffle                    # Toggle shuffle
aux sleep 30m                  # Sleep timer (30m, 1h, 2h, off)

# Library
aux history                    # Show play history
aux history --today            # Today only
aux favorites                  # List favorites (alias: aux fav)
aux fav add <url>              # Add to favorites
aux fav remove <video_id>      # Remove favorite

# Queue
aux queue                      # Show queue (alias: aux q)
aux q add <url>                # Add to queue
aux q next                     # Play next
aux q clear                    # Clear queue

# Playlists
aux playlist list              # List playlists (alias: aux pl)
aux pl create "Chill Vibes"    # Create playlist
aux pl show "Chill Vibes"      # Show items
aux pl add "Chill Vibes" <url> # Add track
aux pl remove "Chill Vibes" <id> # Remove track
aux pl play "Chill Vibes"      # Play all (loads to queue)
aux pl delete "Chill Vibes"    # Delete playlist

# Equalizer
aux eq                         # Show current preset
aux eq bass-boost              # Set preset (flat/bass-boost/vocal/treble/loudness)

# AI
aux chat "summarize this"      # Chat about current video
aux chat --profile deep        # Use specific AI profile
aux suggest                    # AI-powered video suggestions
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
aux chat "summarize this video"
aux chat                         # Interactive mode

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

Press `e` in TUI or use `aux eq <preset>` in CLI. The active preset persists across sessions.

## Configuration

### Player

```bash
aux config player                       # Show current settings
aux config player set --volume 80       # Default volume (0-100)
aux config player set --search-results 10  # Results per search
```

### YouTube

```bash
aux config youtube                      # Show current settings
aux config youtube set --format m4a     # Audio format (m4a, opus, webm)
```

### AI Configuration

#### Interactive Setup (recommended)

```bash
aux config ai --setup        # Guided wizard — pick provider, model, paste API key
```

#### One-liner Setup

```bash
aux config ai set --provider openai --model gpt-4.1-mini --api-key-env OPENAI_API_KEY
aux config ai set --provider openai --model gpt-4.1-mini --api-key sk-xxx
```

#### Profiles

```bash
# Add profiles
aux config ai add-profile deep --provider anthropic --model claude-sonnet-4-6 --api-key-env ANTHROPIC_API_KEY
aux config ai add-profile local --provider ollama --model llama4 --base-url http://localhost:11434

# Use profiles
aux chat "summarize" --profile deep
aux suggest --profile local

# Manage profiles
aux config ai list-profiles
aux config ai remove-profile deep
aux config ai test --profile local
```

#### Custom Providers (OpenAI-compatible)

```bash
aux config ai add-profile groq \
  --provider openai \
  --model llama-3.3-70b \
  --base-url https://api.groq.com/openai/v1 \
  --api-key-env GROQ_API_KEY
```

#### Config File

`~/.config/aux/config.toml`:

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
├── config.rs        # TOML config (~/.config/aux/)
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

| Feature | Spotify | Aux |
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
