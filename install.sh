#!/bin/sh
# aux installer — https://github.com/bonnguyenitc/aux
# Usage: curl -sSL https://raw.githubusercontent.com/bonnguyenitc/aux/main/install.sh | sh
set -e

BOLD='\033[1m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { printf "${CYAN}→${NC} %s\n" "$1"; }
ok()    { printf "${GREEN}✔${NC} %s\n" "$1"; }
warn()  { printf "${YELLOW}!${NC} %s\n" "$1"; }
error() { printf "${RED}✖${NC} %s\n" "$1"; exit 1; }

echo ""
printf "${BOLD}🎵 aux installer${NC}\n"
echo "   AI-powered terminal music player"
echo ""

# ─── Detect OS ──────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) PLATFORM="macOS" ;;
  Linux)  PLATFORM="Linux" ;;
  *)      error "Unsupported OS: $OS. aux supports macOS and Linux." ;;
esac

info "Detected ${PLATFORM} (${ARCH})"

# ─── Check for Rust / Cargo ────────────────────────────────────────────
if command -v cargo >/dev/null 2>&1; then
  ok "Rust/Cargo found: $(cargo --version)"
else
  warn "Rust not found. Installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  . "$HOME/.cargo/env"
  ok "Rust installed: $(cargo --version)"
fi

# ─── Install dependencies ──────────────────────────────────────────────
install_deps_macos() {
  if ! command -v brew >/dev/null 2>&1; then
    error "Homebrew not found. Install it from https://brew.sh first."
  fi

  if ! command -v yt-dlp >/dev/null 2>&1; then
    info "Installing yt-dlp..."
    brew install yt-dlp
    ok "yt-dlp installed"
  else
    ok "yt-dlp found"
  fi

  if ! command -v mpv >/dev/null 2>&1; then
    info "Installing mpv..."
    brew install mpv
    ok "mpv installed"
  else
    ok "mpv found"
  fi
}

install_deps_linux() {
  if command -v apt-get >/dev/null 2>&1; then
    PKG_MGR="apt-get"
    INSTALL_CMD="sudo apt-get install -y"
  elif command -v pacman >/dev/null 2>&1; then
    PKG_MGR="pacman"
    INSTALL_CMD="sudo pacman -S --noconfirm"
  elif command -v dnf >/dev/null 2>&1; then
    PKG_MGR="dnf"
    INSTALL_CMD="sudo dnf install -y"
  else
    warn "Could not detect package manager. Please install yt-dlp and mpv manually."
    return
  fi

  if ! command -v yt-dlp >/dev/null 2>&1; then
    info "Installing yt-dlp via ${PKG_MGR}..."
    $INSTALL_CMD yt-dlp || warn "Failed to install yt-dlp via ${PKG_MGR}. Try: pip install yt-dlp"
  else
    ok "yt-dlp found"
  fi

  if ! command -v mpv >/dev/null 2>&1; then
    info "Installing mpv via ${PKG_MGR}..."
    $INSTALL_CMD mpv
    ok "mpv installed"
  else
    ok "mpv found"
  fi
}

case "$PLATFORM" in
  macOS) install_deps_macos ;;
  Linux) install_deps_linux ;;
esac

# ─── Install aux ────────────────────────────────────────────────────────
info "Installing aux from crates.io..."

if cargo install aux 2>/dev/null; then
  ok "aux installed successfully"
else
  info "Falling back to building from source..."
  TMPDIR=$(mktemp -d)
  git clone --depth 1 https://github.com/bonnguyenitc/aux.git "$TMPDIR/aux"
  cargo install --path "$TMPDIR/aux"
  rm -rf "$TMPDIR"
  ok "aux built and installed from source"
fi

# ─── Done ───────────────────────────────────────────────────────────────
echo ""
printf "${BOLD}${GREEN}🎵 aux is ready!${NC}\n"
echo ""
echo "   Get started:"
echo "   ┌──────────────────────────────────────────┐"
echo "   │  aux                    # launch player   │"
echo "   │  aux chat \"play lofi\"   # AI plays music  │"
echo "   │  aux config ai --setup  # setup AI key    │"
echo "   └──────────────────────────────────────────┘"
echo ""
echo "   Star us on GitHub: https://github.com/bonnguyenitc/aux"
echo ""
