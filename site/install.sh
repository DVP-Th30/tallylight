#!/bin/sh
# tallylight installer: downloads the latest release for this machine and puts it in ~/.local/bin.
#   curl -fsSL https://dvp-th30.github.io/tallylight/install.sh | sh
# Set TALLYLIGHT_VERSION=v0.1.0 to pin a version, TALLYLIGHT_BIN_DIR to choose the folder.
set -eu

REPO="DVP-Th30/tallylight"
BIN_DIR="${TALLYLIGHT_BIN_DIR:-$HOME/.local/bin}"

fail() { echo "tallylight: $1" >&2; exit 1; }

case "$(uname -s)" in
  Linux) os="unknown-linux-musl" ;;
  Darwin) os="apple-darwin" ;;
  *) fail "unsupported system: $(uname -s). Linux and macOS only for now." ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) fail "unsupported processor: $(uname -m)" ;;
esac
target="$arch-$os"
archive="tallylight-$target.tar.gz"

if [ -n "${TALLYLIGHT_VERSION:-}" ]; then
  base="https://github.com/$REPO/releases/download/$TALLYLIGHT_VERSION"
else
  base="https://github.com/$REPO/releases/latest/download"
fi

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $archive"
curl -fsSL "$base/$archive" -o "$tmp/$archive" || fail "download failed: $base/$archive"
curl -fsSL "$base/$archive.sha256" -o "$tmp/$archive.sha256" || fail "checksum download failed"

expected="$(cut -d ' ' -f 1 "$tmp/$archive.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$archive" | cut -d ' ' -f 1)"
else
  actual="$(shasum -a 256 "$tmp/$archive" | cut -d ' ' -f 1)"
fi
[ "$expected" = "$actual" ] || fail "checksum mismatch, nothing installed"

tar xzf "$tmp/$archive" -C "$tmp"
mkdir -p "$BIN_DIR"
install -m 755 "$tmp/tallylight-$target/tallylight" "$BIN_DIR/tallylight"

echo "Installed $("$BIN_DIR/tallylight" --version) in $BIN_DIR"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "Note: $BIN_DIR is not in your PATH. Add it, or call $BIN_DIR/tallylight directly." ;;
esac
echo
echo "Next:"
echo "  tallylight install    hooks Claude Code and starts the background service"
echo "  tallylight pair       shows a QR code to pair a screen"
