# Changelog

## v0.2.0 (2026-11-10)

- Rewritten in Rust: one ~600 KB binary instead of a Python script, same card, same config and
  service. `install.sh` now builds it with cargo and restarts the service.
- Unit tests for the activity card, the Game State Integration listener and the Discord IPC
  protocol, plus a CI workflow (fmt, clippy, tests).
- `cs2-presence --version`.

## v0.1.0 (2026-11-04)

- Discord Rich Presence for native Linux CS2 from Game State Integration: mode, map, score, round,
  team, K/A/D, MVPs, health, armour, money, map pictures and team badges.
