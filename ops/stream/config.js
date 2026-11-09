// Everything you'd want to change between streams lives here.
// Edit, save, then right-click the source in OBS -> Refresh (or toggle the scene).
window.ONESHOT = {
  // Shown on the Starting Soon and BRB cards.
  today: "Duel ladder climb, then Big Team with chat",

  // Starting Soon countdown, in minutes. Override per-source with ?min=10
  // or pin it to a wall-clock time with ?until=19:30 (24h, local time).
  countdownMinutes: 5,

  playUrl: "elopros.com/games/oneshot",
  site: "elopros.com",
  socials: [
    ["X", "@ELOonSOL"],
    ["Discord", "discord.gg/UYg2D3Yudm"],
    ["Telegram", "t.me/eloonsol"],
  ],

  // Scrolling ticker lines. Keep them short; they loop forever.
  ticker: [
    "PLAY FREE → elopros.com/games/oneshot — in the browser or on desktop",
    "Shields · power weapons on pads · Big Team 8v8 with trucks · ranked on Elo Pros",
    "Fifteen guns · seven maps · Duel, Slayer, Gun Game, Capture the Flag",
    "Discord → discord.gg/UYg2D3Yudm",
    "X → @ELOonSOL  ·  Telegram → t.me/eloonsol",
  ],

  // Live prices from DexScreener (free, no key), any chain. Set to [] to hide.
  // A coin quoted in one listed above it shows that price and ≈$.
  tokens: [
    { symbol: "PROS", address: "2Uwte4c9ZnRACas6Miq6bR8zx8n8Psgu11Vn5htHpump" },
  ],

  // Background slideshow on the full-screen cards.
  art: [
    "art/bigteam.jpg",
    "art/scope.jpg",
    "art/minigun.jpg",
    "art/chrysalis.jpg",
    "art/cloudbreak.jpg",
    "art/dryfork.jpg",
    "art/fighter.jpg",
    "art/bulkhead.jpg",
    "art/overpass.jpg",
    "art/proving.jpg",
    "art/cistern.jpg",
  ],

  // The game itself watching a live match a little behind, for the
  // "Watch a match" scene (?watch=1, or a party's ?watch=CODE).
  watchUrl: "https://elopros.com/games/oneshot/play/?watch=1&gfx=webgl2&quality=low&streamer=1",
};
