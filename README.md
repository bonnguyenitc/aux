# aux 🎵

> **Listen to music with your AI agent — in the terminal.**

Stop alt-tabbing to Spotify. Stop feeding Chrome 2GB of RAM for a YouTube tab. Stop losing your flow to ads and recommendations rabbit holes.

aux is an open-source terminal music player that pairs you with an AI companion. Search YouTube, SoundCloud, YT Music & [1000+ sites](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md) — no browser, no ads, no distractions. Get synced lyrics, chat about what you're hearing, and let AI suggest your next track. Built in Rust. Zero subscriptions.

![aux demo](demo/demo-aux.gif)

## ⚡ Install

```bash
brew install yt-dlp mpv        # dependencies
cargo install --path .         # install aux
```

## 🔥 Why developers love aux

- 🤖 **AI listens with you** — ask "what's this song about?", get podcast summaries, receive smart recommendations
- 🎧 **1000+ sources** — YouTube, SoundCloud, YT Music, Bandcamp, and more — no browser, no ads
- 📝 **Auto-synced lyrics** — karaoke mode in your terminal
- 🎛️ **Full player** — queue, playlists, EQ, shuffle, repeat, sleep timer
- ⌨️ **Keyboard-only** — never leave your workflow

## 🎬 Quick start

```bash
aux                                  # launch TUI (9 panels, full control)
aux search "lofi coding music"       # search & play from CLI
aux play <url>                       # play any URL
```

**TUI panels:** Search → Results → Lyrics → Queue → Favorites → History → Playlists → Chat → Help

Press `Tab` to switch panels, `?` for help.

## 🤖 Your AI music companion

aux isn't just a player — it's a **listening partner**. Transcripts are fetched automatically so AI understands what you're hearing.

```bash
aux chat "summarize this podcast"     # get the gist without rewinding
aux chat "what are the lyrics about?" # understand any song
aux chat "recommend something chill"  # conversational discovery
aux suggest                           # AI picks your next track
aux chat                              # open-ended conversation
```

**Works with your favorite LLM** — OpenAI, Anthropic, Gemini, or local Ollama:

```bash
aux config ai --setup                 # 30-second guided wizard
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

## 🎵 CLI commands

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

# AI profiles
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

## 🏆 aux vs Spotify

| | Spotify | aux |
|--|---------|-----|
| Search & play | ✅ | ✅ |
| Queue, playlists, favorites | ✅ | ✅ |
| Shuffle, repeat, EQ, sleep timer | ✅ | ✅ |
| Synced lyrics | ✅ | ✅ |
| **AI chat companion** | ❌ | ✅ |
| **Terminal / keyboard-only** | ❌ | ✅ |
| **Open source** | ❌ | ✅ |
| **Free forever** | ❌ | ✅ |
| **1000+ audio sources** | ❌ | ✅ |
| Multi-device | ✅ | ❌ |
| Offline downloads | ✅ | ❌ |

## 🛠️ Built with

Rust · [ratatui](https://github.com/ratatui/ratatui) · [mpv](https://mpv.io/) · [yt-dlp](https://github.com/yt-dlp/yt-dlp) · SQLite

## License

MIT
