# cs2-presence

Discord doesn't recognise the **native Linux build of Counter-Strike 2**, so your status stays empty
while you play. cs2-presence fixes that: while `cs2` is running, Discord shows **Playing
Counter-Strike 2** with the official CS2 icon and how long you've been playing. When the game closes,
the status clears.

- One small Python script, standard library only, no Discord token or login: it talks to the Discord
  client on your own machine through its local RPC socket, the way games do.
- Works with the Discord client, Vesktop/arRPC, and the Flatpak or Snap Discord.
- Checks every 5 seconds whether `cs2` runs as your user. It costs nothing while you aren't playing.

## Install

```sh
git clone https://github.com/Tjabooo/cs2-presence.git
cd cs2-presence
./install.sh
```

This puts `cs2-presence` in `~/.local/bin` and enables a systemd user service that starts with your
graphical session. Logs: `journalctl --user -u cs2-presence`.

Discord must allow it: User Settings → Activity Privacy → **Share your detected activities with
others** on.

Remove: `systemctl --user disable --now cs2-presence && rm ~/.local/bin/cs2-presence ~/.config/systemd/user/cs2-presence.service`

## How it works

It finds a process whose `comm` and executable are both `cs2` and that is owned by you, and takes
the game's start time from `/proc`. It then connects to the first Discord IPC socket it finds
(`$XDG_RUNTIME_DIR/discord-ipc-*`, or inside `app/*/` for Flatpak) and sends `SET_ACTIVITY` with
Discord's own Counter-Strike 2 application ID. The activity is re-sent every 5 s, so it survives
Discord restarting, and it's cleared when the game exits or the service stops.

## License

MIT
