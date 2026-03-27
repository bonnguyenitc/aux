# Contributing to aux 🎵

Thanks for wanting to help! aux is open to contributions of all kinds.

## Quick Start

```bash
# Clone & build
git clone https://github.com/bonnguyenitc/aux.git
cd duet
brew install yt-dlp mpv    # macOS
cargo build

# Run tests
cargo test

# Run locally
cargo run
```

## Ways to Contribute

- 🐛 **Bug reports** — [Open an issue](https://github.com/bonnguyenitc/aux/issues)
- 💡 **Feature ideas** — Start a [Discussion](https://github.com/bonnguyenitc/aux/discussions)
- 🔧 **Code** — Pick an issue labeled `good first issue` and submit a PR
- 📖 **Docs** — Improve README, add examples, fix typos

## Pull Request Process

1. Fork the repo
2. Create a branch (`git checkout -b feat/my-feature`)
3. Make changes, run `cargo test` and `cargo clippy`
4. Submit a PR with a clear description

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` — treat warnings as errors
- Write tests for new functionality
- Keep PRs focused — one feature or fix per PR

## Project Structure

```
src/
├── main.rs          # CLI + TUI entry point
├── ai/              # AI chat, action pipeline
├── media/           # yt-dlp backend, types
├── player/          # mpv player control
├── library/         # favorites, queue, playlists, history
└── tui/             # ratatui UI components
```

## License

By contributing, you agree that your contributions will be licensed under MIT.
