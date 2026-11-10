#!/bin/sh
# Build cs2-presence, install it for this user and start it with your graphical session.
set -eu
cd "$(dirname "$0")"
if ! command -v cargo >/dev/null 2>&1; then
    echo "cs2-presence needs Rust to build: install it from your package manager or https://rustup.rs" >&2
    exit 1
fi
cargo build --release --locked
install -Dm755 target/release/cs2-presence "$HOME/.local/bin/cs2-presence"
install -Dm644 cs2-presence.service "$HOME/.config/systemd/user/cs2-presence.service"
systemctl --user daemon-reload
systemctl --user enable cs2-presence.service
systemctl --user restart cs2-presence.service
echo "cs2-presence is running: start CS2 and check your Discord status (journalctl --user -u cs2-presence)."
