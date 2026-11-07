# oneshot for the desktop

Unpack it anywhere and run `oneshot` (`oneshot.exe` on Windows). Keep the
`assets` folder beside it.

```
oneshot                          the title screen: pick a mode, PLAY, on the public server
oneshot --name you --mode ffa    straight into a match: ffa, gungame, bigteam or casual (you and bots)
oneshot --server 127.0.0.1:8080  play on a local oneshot-server
oneshot --mode watch --party CODE   watch a party's match through a player's eyes, a little behind
oneshot --streamer               streamer mode this run: the party code and your wallet stay off the screen
oneshot --lite                   the light tier, for a slow GPU
oneshot --help                   everything else, and the keys
```

Click the window to take the mouse; Esc opens the menu (settings, leave the
match). F9 saves the last half minute as a video (with ffmpeg on the path)
and F12 a screenshot, both into your pictures folder under oneshot/. For a
stream, SETTINGS > STREAM; the overlays, OBS scenes and chat commands are at
elopros.com/stream.html.

- **macOS:** the build isn't signed yet. The first time, right-click
  `oneshot`, Open, and confirm (or `xattr -dr com.apple.quarantine .` in
  this folder).
- **Linux:** X11 or Wayland, and a Vulkan driver (`mesa-vulkan-drivers` on
  most distributions).

Ranked needs an Elo Pros sign-in, which is on the web page for now
(elopros.com/games/oneshot/play); the desktop client plays every casual mode.
