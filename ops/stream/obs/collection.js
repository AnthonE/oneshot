// The oneshot OBS scene collection, for wherever this folder lives.
// make.html calls it in a browser (it knows its own folder); node writes the
// checked-in obs/oneshot.json:
//   node ops/stream/obs/collection.js file:///home/anthony/stream-overlays/oneshot/ linux > ops/stream/obs/oneshot.json
// Same shape as the collection elopros.com/stream.html writes (scry-forge
// watchtower/js/stream.js), which OBS imports.
(function (root) {
  let n = 0;
  // Stable ids, so a re-made file diffs cleanly.
  const uuid = () => "0e5f0000-0000-4000-8000-" + String(++n).padStart(12, "0");
  const BROWSER_CSS = "body { background-color: rgba(0, 0, 0, 0); margin: 0px auto; overflow: hidden; }";
  const CAPTURE = {
    windows: { id: "game_capture", name: "Game Window (point it at oneshot)", settings: { capture_mode: "any_fullscreen" } },
    mac: { id: "screen_capture", name: "Game Window (pick the oneshot window)", settings: { type: 1 } },
    linux: { id: "xcomposite_input", name: "Game Window (pick the oneshot window)", settings: {} },
  };

  function source(id, name, settings) {
    return {
      name, uuid: uuid(), id, versioned_id: id, settings,
      mixers: 255, sync: 0, flags: 0, volume: 1.0, balance: 0.5, enabled: true, muted: false,
      "push-to-mute": false, "push-to-mute-delay": 0, "push-to-talk": false, "push-to-talk-delay": 0,
      hotkeys: {}, deinterlace_mode: 0, deinterlace_field_order: 0, monitoring_type: 0, private_settings: {},
    };
  }
  function browser(name, url, w, h, audio) {
    return source("browser_source", name, {
      url, width: w, height: h, fps: 30, fps_custom: true, shutdown: false,
      restart_when_active: false, css: BROWSER_CSS, reroute_audio: !!audio,
    });
  }
  function item(src, id, at = {}) {
    return {
      name: src.name, source_uuid: src.uuid, visible: at.visible !== false, locked: !!at.locked,
      rot: 0.0, pos: { x: at.x || 0, y: at.y || 0 }, scale: { x: at.s || 1, y: at.s || 1 },
      align: 5, bounds_type: 0, bounds_align: 0, bounds: { x: 0, y: 0 }, bounds_crop: false,
      crop_left: 0, crop_top: 0, crop_right: 0, crop_bottom: 0, id, group_item_backup: false,
      scale_filter: "disable", blend_method: "default", blend_type: "normal",
      show_transition: { duration: 0 }, hide_transition: { duration: 0 }, private_settings: {},
    };
  }
  // the bottom of the list draws on top, as OBS stacks a scene
  function scene(name, items) {
    const s = source("scene", name, { id_counter: items.length, custom_size: false, items: [] });
    s.settings.items = items.map((x, i) => item(x[0], i + 1, x[1]));
    return s;
  }

  // base: this folder as a file:// URL ending in "/"; os: windows | mac | linux;
  // watchUrl: config.js's, the game watching a live match.
  function collection(base, os, watchUrl) {
    n = 0;
    const cap = CAPTURE[os] || CAPTURE.windows;
    const capture = source(cap.id, cap.name, cap.settings);
    const b = {
      start: browser("Card - Starting", base + "card.html?m=start", 1920, 1080),
      brb: browser("Card - BRB", base + "card.html?m=brb", 1920, 1080),
      end: browser("Card - Ending", base + "card.html?m=end", 1920, 1080),
      bug: browser("Overlay - URL Bug", base + "bug.html", 1920, 1080),
      cam: browser("Frame - Cam", base + "frame.html?label=CAM&name=anthony", 480, 284),
      chat: browser("Frame - Chat", base + "frame.html?label=COMMS&name=chat&panel=1", 480, 600),
      alerts: browser("Frame - Alerts", base + "frame.html?label=INCOMING&name=alerts", 600, 200),
      watch: browser("Match cam (the game, watching)", watchUrl, 1920, 1080, true),
    };
    const R = 1920 - 28, B = 1080 - 28, T = 52; // the ticker's height
    const scenes = [
      scene("1 - Starting Soon", [[b.start, { locked: true }]]),
      scene("2 - Game", [
        [capture],
        [b.chat, { visible: false, x: R - 480, y: B - T - 600 + 28 }],
        [b.cam, { visible: false, x: 28, y: B - T - 284 }],
        [b.alerts, { visible: false, x: (1920 - 600) / 2, y: 120 }],
        [b.bug, { locked: true }],
      ]),
      scene("3 - BRB", [[b.brb, { locked: true }]]),
      scene("4 - Ending", [[b.end, { locked: true }]]),
      scene("5 - Watch a match", [
        [b.watch],
        [b.cam, { visible: false, x: 28, y: B - T - 284 }],
        [b.bug, { locked: true }],
      ]),
    ];
    return {
      name: "oneshot",
      current_scene: "1 - Starting Soon",
      current_program_scene: "1 - Starting Soon",
      scene_order: scenes.map((s) => ({ name: s.name })),
      sources: [capture, ...Object.values(b), ...scenes],
      groups: [],
      quick_transitions: [
        { name: "Cut", duration: 300, hotkeys: [], id: 1, fade_to_black: false },
        { name: "Fade", duration: 300, hotkeys: [], id: 2, fade_to_black: false },
      ],
      transitions: [],
      current_transition: "Fade",
      transition_duration: 300,
      preview_locked: false,
      scaling_enabled: false,
      resolution: { x: 1920, y: 1080 },
      version: 2,
      modules: {},
    };
  }

  if (typeof module !== "undefined" && module.exports) {
    module.exports = collection;
    if (require.main === module) {
      const path = require("path"), fs = require("fs"), vm = require("vm");
      const box = { window: {} };
      vm.runInNewContext(fs.readFileSync(path.join(__dirname, "..", "config.js"), "utf8"), box);
      const base = process.argv[2] || "file:///home/anthony/stream-overlays/oneshot/";
      const os = process.argv[3] || "linux";
      process.stdout.write(JSON.stringify(collection(base.endsWith("/") ? base : base + "/", os, box.window.ONESHOT.watchUrl), null, 2) + "\n");
    }
  } else {
    root.oneshotCollection = collection;
  }
})(this);
