# oneshot stream overlays

Ported from Gates' `ops/stream`. Plain files, no server: OBS loads them
straight off disk.

Unzip it (to `~/stream-overlays/oneshot/` and `obs/oneshot.json` already
points there), then open `obs/make.html` in a browser: it writes the OBS
scene collection for wherever the folder is, and lists every overlay's link.
In OBS: Scene Collection → Import, then switch to `oneshot`.

Edit `config.js` (today's agenda, countdown minutes, ticker lines, socials,
prices), save, then right-click the source in OBS → Refresh.

| OBS source | file, in the browser source's URL field (not "Local file") | size |
|---|---|---|
| Card - Starting | `card.html?m=start` (add `&min=10` or `&until=19:30`) | 1920×1080 |
| Card - BRB | `card.html?m=brb` | 1920×1080 |
| Card - Ending | `card.html?m=end` | 1920×1080 |
| Overlay - URL Bug | `bug.html` (`?ticker=0` hides the ticker, `?corner=tr` moves the bug, `?scale=0.8`) | 1920×1080 |
| Frame - Cam | `frame.html?label=CAM&name=anthony`, above the cam | the cam's |
| Frame - Chat | `frame.html?label=COMMS&name=chat&panel=1`, below the chat | the chat's |
| Frame - Alerts | `frame.html?label=INCOMING&name=alerts` | the alert's |
| Match cam | `config.js`'s `watchUrl`: the game watching a live match a little behind (`?watch=CODE` for a party's) | 1920×1080, audio to OBS |

Prices: DexScreener's public API, every 60 s, any chain (PROS on Solana).

In the game: SETTINGS > STREAM's streamer mode keeps the party code and the
wallet off the screen (`?streamer=1`, `--streamer`); the PARTY tab's COPY
WATCH LINK lets viewers watch your match; F9 saves the last half minute as a
video, F12 a screenshot. The hosted overlays and chat commands are on
`elopros.com/stream.html` (pick oneshot).

`obs/oneshot.json` is made by `node ops/stream/obs/collection.js
[file:///…/oneshot/] [windows|mac|linux] > ops/stream/obs/oneshot.json`.
The zip: `cd ops && zip -r ../oneshot-stream.zip stream`. Art is the store's
stills of the game; fonts are the game's (CC0, `fonts/LICENSE-CC0.txt`).
Profiles (stream keys) are deliberately not here.
