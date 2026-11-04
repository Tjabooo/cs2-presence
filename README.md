# cs2-presence

Discord doesn't recognise the **native Linux build of Counter-Strike 2**, so your status stays empty
while you play. cs2-presence fixes that with a full Rich Presence card under **Playing
Counter-Strike 2**:

| Where | What it shows |
| --- | --- |
| Details | Mode, map and the score from your side: `Competitive · Dust II · 7–5` |
| State | Your K/A/D and MVPs, or kills/deaths/points in Deathmatch; Warmup, Halftime, Won/Lost, Overtime |
| Party | Round progress in Competitive, Premier and Wingman: `(13 of 24)` |
| Big picture | The map, with the map file, round, freeze time and bomb status on hover |
| Small picture | Your team (CT/T), with health, armour, helmet and money on hover, or who you're watching when dead |
| Timer | Time on the current map (time since launch in the menu) |
| Button | Your Steam profile (others see it, you don't) |

In the main menu it shows `In the main menu` and your in-game name. When the game closes, the status
clears.

- One small Python script, standard library only, no Discord token or login: it talks to the Discord
  client on your own machine through its local RPC socket, the way games do.
- Match details come from CS2's own **Game State Integration**, the Valve-supported feed that
  tournament overlays use. It isn't a cheat and doesn't touch the game's memory.
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

Start the service before CS2 (it writes the Game State Integration config, which the game reads at
launch). If CS2 was already running when you installed, restart the game once to get match details.

Discord must allow it: User Settings → Activity Privacy → **Share your detected activities with
others** on. Discord shows only one game activity at a time, the one that connected first: if another
game (for example Roblox through Sober) is already showing, CS2 stays hidden until that one closes.

Remove: `systemctl --user disable --now cs2-presence && rm ~/.local/bin/cs2-presence ~/.config/systemd/user/cs2-presence.service`,
then delete `gamestate_integration_cs2_presence.cfg` from CS2's `game/csgo/cfg` folder.

## How it works

It finds a process whose `comm` and executable are both `cs2` and that is owned by you, and takes
the game's start time from `/proc`.

At startup it writes `gamestate_integration_cs2_presence.cfg` into CS2's `game/csgo/cfg` folder in
every Steam library it finds (native, Flatpak and Snap Steam). CS2 then posts the game state (map,
round, your player) to `http://127.0.0.1:3713/` about once a second while it changes; set
`CS2_PRESENCE_PORT` in the service to use another port. Your own stats are remembered while the game
reports the player you're spectating.

It connects to the first Discord IPC socket it finds (`$XDG_RUNTIME_DIR/discord-ipc-*`, or inside
`app/*/` for Flatpak, `snap.discord*/` for Snap) and sends `SET_ACTIVITY` with Discord's own
Counter-Strike 2 application ID. Updates go out when something changes, at most every 4 s (Discord's
limit is 5 per 20 s), and every 30 s regardless, so the activity survives Discord restarting. It's
cleared when the game exits or the service stops.

Map pictures come from [ghostcap-gaming/cs2-map-images](https://github.com/ghostcap-gaming/cs2-map-images);
maps without one show the CS2 icon. The team badges are in `assets/`.

## License

MIT
