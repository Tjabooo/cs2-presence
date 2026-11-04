#!/bin/sh
# Install cs2-presence for this user and start it with your graphical session.
set -eu
cd "$(dirname "$0")"
install -Dm755 cs2-presence "$HOME/.local/bin/cs2-presence"
install -Dm644 cs2-presence.service "$HOME/.config/systemd/user/cs2-presence.service"
systemctl --user daemon-reload
systemctl --user enable --now cs2-presence.service
echo "cs2-presence is running: start CS2 and check your Discord status (journalctl --user -u cs2-presence)."
