# aux 🎵

[![GitHub stars](https://img.shields.io/github/stars/bonnguyenitc/duet?style=social)](https://github.com/bonnguyenitc/duet)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-dea584.svg)](https://www.rust-lang.org/)

> **Your terminal music player — with an AI that actually listens.**

No browser tabs. No ads. No 2GB RAM drain. Just music and an AI companion that understands what you're hearing.

Search YouTube, SoundCloud, YT Music & [1000+ sources](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md). Get synced lyrics. Chat about any song. Let AI control the player for you — all from the terminal. Built in Rust. Free forever.

![aux demo](demo/demo-aux.gif)

## ⚡ Install

```bash
brew install yt-dlp mpv        # dependencies
cargo install --path .         # install aux
```

## 🔥 Why devs switch to aux

- 🤖 **Talk to your player** — "play something chill", "skip to the chorus", "add to favorites" — your AI handles it
- 🎧 **1000+ sources, zero ads** — YouTube, SoundCloud, Bandcamp, and more — no account needed
- 📝 **Auto-synced lyrics** — karaoke mode, right in the terminal
- 🎛️ **Full-featured** — queue, playlists, favorites, EQ, shuffle, repeat, sleep timer
- ⌨️ **Keyboard-only** — never leave your workflow

## 🎬 Quick start

```bash
aux                                  # launch TUI (9 panels, full control)
aux search "lofi coding music"       # search & pick from CLI
aux play <url>                       # play any URL directly
```

Press `Tab` to switch panels, `?` for all keybindings.

## 🤖 AI-powered — just chat

aux isn't just a player. It's a **listening partner** that understands context, executes commands, and chains actions — all through natural language.

### Talk, don't type commands

Open the Chat panel (`c`) and type what you want:

```
"play a random song by Ed Sheeran"        → searches + plays random result
"add this to my Chill playlist"            → adds current track
"next track"                               → skips to next
"pause"                                    → pauses
"what is this song about?"                 → explains the song
```

### What AI can do

| Category | Examples |
|----------|----------|
| **Search & Play** | Search, play by name, pick from results, play random |
| **Playback** | Pause, resume, seek, volume, speed, repeat, shuffle, sleep timer |
| **Library** | Add/remove favorites, add to queue, clear queue |
| **Playlists** | Create, delete, add tracks, play entire playlist |
| **Navigation** | Switch to any panel (queue, favorites, history, lyrics...) |
| **Compose** | Chain actions: *"search lofi and play the 3rd result"* |

### CLI chat

```bash
aux chat "summarize this podcast"     # quick one-shot
aux chat "recommend something chill"  # AI picks your next track
aux chat                              # open interactive chat
aux suggest                           # AI suggests related tracks
```

### Setup (30 seconds)

Works with **OpenAI**, **Anthropic**, **Google Gemini**, or local **Ollama**:

```bash
aux config ai --setup                 # guided wizard
```

## ⌨️ Keybindings

| Key | Action | | Key | Action |
|-----|--------|--|-----|--------|
| `Space` | Pause / Resume | | `f` | Toggle favorite ❤️ |
| `←` `→` | Seek ±10s | | `a` | Add to queue 📋 |
| `+` `-` | Volume ±5% | | `n` / `p` | Next / Previous |
| `]` `[` | Speed up / down | | `r` | Cycle repeat |
| `/` | New search | | `z` | Toggle shuffle 🔀 |
| `e` | Cycle EQ preset 🎛️ | | `t` | Sleep timer 😴 |
| `c` | Open chat 💬 | | `?` | Help |

## 🎵 CLI reference

<details>
<summary><b>Playback</b></summary>

```bash
aux pause / resume / stop
aux now                              # what's playing
aux volume 80                        # set volume
aux seek +30 / -10 / 2:30            # seek
aux speed 1.5 / up / down            # playback speed
aux repeat off / one / all
aux shuffle
aux sleep 30m / 1h / off
```

</details>

<details>
<summary><b>Library</b></summary>

```bash
aux history                          # play history
aux favorites                        # list favorites (alias: aux fav)
aux fav add <url>                    # add to favorites
```

</details>

<details>
<summary><b>Queue & Playlists</b></summary>

```bash
aux queue                            # show queue (alias: aux q)
aux q add <url>                      # add to queue

aux playlist list                    # list playlists (alias: aux pl)
aux pl create "Chill Vibes"          # create playlist
aux pl play "Chill Vibes"            # play all
aux pl add "Chill Vibes" <url>       # add track
```

</details>

<details>
<summary><b>Equalizer</b></summary>

Five presets: `flat` · `bass-boost` · `vocal` · `treble` · `loudness`

```bash
aux eq                               # show current
aux eq bass-boost                    # set preset
```

</details>

<details>
<summary><b>Configuration</b></summary>

```bash
aux config player                    # show player settings
aux config player set --volume 80    # default volume
aux config ai --setup                # AI setup wizard

# Multiple AI profiles
aux config ai add-profile deep \
  --provider anthropic \
  --model claude-sonnet-4-6 \
  --api-key-env ANTHROPIC_API_KEY

aux config ai add-profile local \
  --provider ollama \
  --model llama4 \
  --base-url http://localhost:11434
```

Config file: `~/.config/aux/config.toml`

</details>

## 🏆 aux vs the rest

| | Spotify | YouTube | aux |
|--|---------|---------|-----|
| Search & play | ✅ | ✅ | ✅ |
| Queue, playlists, favorites | ✅ | ✅ | ✅ |
| Shuffle, repeat, EQ, sleep timer | ✅ | ✅ | ✅ |
| Synced lyrics | ✅ | ❌ | ✅ |
| **AI chat companion** | ❌ | ❌ | ✅ |
| **Natural language control** | ❌ | ❌ | ✅ |
| **Terminal / keyboard-only** | ❌ | ❌ | ✅ |
| **1000+ audio sources** | ❌ | ❌ | ✅ |
| **Open source** | ❌ | ❌ | ✅ |
| **Free forever** | ❌ | ❌ | ✅ |
| Multi-device sync | ✅ | ✅ | ❌ |
| Offline downloads | ✅ | ✅ | ❌ |

## 🛠️ Built with

Rust · [ratatui](https://github.com/ratatui/ratatui) · [mpv](https://mpv.io/) · [yt-dlp](https://github.com/yt-dlp/yt-dlp) · SQLite

## License

MIT
