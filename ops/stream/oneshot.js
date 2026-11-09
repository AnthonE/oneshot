// Shared behaviour: the mark, the ticker (with live DexScreener prices), and
// the tracers crossing the full-screen cards. Every overlay loads config.js first.
(function () {
  const C = window.ONESHOT;

  // The reticle, the game's mark (tools/logo, icon-a-reticle).
  const MARK = '<svg viewBox="0 0 512 512"><rect width="512" height="512" rx="96" fill="#1b1916"/>' +
    '<g stroke="#e8b931" fill="none"><circle cx="256" cy="256" r="160" stroke-width="34"/>' +
    '<path stroke-width="34" d="M256 79V96M433 256H416M256 433V416M79 256H96"/>' +
    '<path stroke-width="28" d="M256 106V164M406 256H348M256 406V348M106 256H164"/></g>' +
    '<circle cx="256" cy="256" r="22" fill="#e8b931"/></svg>';
  document.querySelectorAll(".mark").forEach((el) => (el.innerHTML = MARK));

  const esc = (s) => String(s).replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);

  // ---- ticker ----
  window.mountTicker = function (host) {
    host.classList.add("ticker");
    host.innerHTML =
      '<div class="prices"></div><div class="lane"><div class="belt"></div></div>' +
      '<div class="end">▶ ' + esc(C.playUrl) + "</div>";
    if (!C.tokens.length) host.querySelector(".prices").remove();

    // Two identical copies of the message run, so the loop seam is invisible.
    const belt = host.querySelector(".belt");
    const run = C.ticker.map((t) => "<span>" + esc(t) + "</span><em>◆</em>").join("");
    belt.innerHTML = run + run;
    let x = 0, last = performance.now();
    (function step(now) {
      x -= ((now - last) / 1000) * 70; // px per second
      last = now;
      const half = belt.scrollWidth / 2;
      if (half && -x >= half) x += half;
      belt.style.transform = "translateX(" + x + "px)";
      requestAnimationFrame(step);
    })(last);

    if (C.tokens.length) refreshPrices(host.querySelector(".prices"));
  };

  // Small-cap prices need significant digits, not cents.
  function fmt(n) {
    if (n == null || isNaN(n)) return "—";
    if (n >= 1) return n.toLocaleString("en-US", { maximumFractionDigits: 4 });
    const d = Math.max(2, -Math.floor(Math.log10(n)) + 2);
    return n.toFixed(Math.min(d, 10));
  }
  function chg(p) {
    if (p == null) return "";
    const up = p >= 0;
    return '<span class="chg ' + (up ? "up" : "down") + '">' + (up ? "▲" : "▼") + Math.abs(p).toFixed(1) + "%</span>";
  }

  async function refreshPrices(box) {
    const rows = [];
    const usdOf = {}; // symbol -> $, for a coin quoted in another listed one
    for (const t of C.tokens) {
      try {
        const r = await fetch("https://api.dexscreener.com/latest/dex/tokens/" + t.address);
        const pairs = ((await r.json()).pairs || []).filter(
          (p) => p.baseToken.address.toLowerCase() === t.address.toLowerCase()
        );
        // Most liquid pair where this token is the base.
        pairs.sort((a, b) => ((b.liquidity || {}).usd || 0) - ((a.liquidity || {}).usd || 0));
        const p = pairs[0];
        if (!p) continue;
        const ch = (p.priceChange || {}).h24;
        if (p.priceUsd) {
          const usd = parseFloat(p.priceUsd);
          usdOf[t.symbol] = usd;
          rows.push('<div class="px"><b>' + esc(t.symbol) + "</b>$" + fmt(usd) + chg(ch) + "</div>");
        } else {
          // Quoted in another token: show both.
          const nat = parseFloat(p.priceNative);
          const q = p.quoteToken.symbol;
          const usd = usdOf[q] ? " ≈$" + fmt(nat * usdOf[q]) : "";
          rows.push('<div class="px"><b>' + esc(t.symbol) + "</b>" + fmt(nat) + " " + esc(q) +
            '<span class="sub">' + usd + "</span>" + chg(ch) + "</div>");
        }
      } catch (e) { /* offline or rate-limited: keep the last good numbers */ }
    }
    if (rows.length) box.innerHTML = rows.join("");
    setTimeout(() => refreshPrices(box), 60000);
  }

  // ---- tracers: brass streaks crossing the frame, now and then, over slow dust ----
  window.mountTracers = function (canvas, dust) {
    const ctx = canvas.getContext("2d");
    const W = (canvas.width = canvas.offsetWidth), H = (canvas.height = canvas.offsetHeight);
    const motes = Array.from({ length: dust }, () => ({
      x: Math.random() * W, y: Math.random() * H, r: 0.6 + Math.random() * 1.6,
      vx: 4 + Math.random() * 10, vy: -(2 + Math.random() * 6), a: 0.12 + Math.random() * 0.3, ph: Math.random() * 6.28,
    }));
    const shots = [];
    let wait = 0.8;
    function fire() {
      // a shallow line from off the left edge, across, sometimes a burst of three
      const y = H * (0.12 + Math.random() * 0.76), tilt = (Math.random() - 0.5) * 0.22;
      const n = Math.random() < 0.35 ? 3 : 1, speed = 2600 + Math.random() * 1400;
      for (let i = 0; i < n; i++) {
        shots.push({ x: -200 - i * 260, y: y + i * 6, dx: Math.cos(tilt), dy: Math.sin(tilt), v: speed, len: 160 + Math.random() * 140 });
      }
      wait = 1.4 + Math.random() * 3.2;
    }
    let last = performance.now();
    (function frame(now) {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      ctx.clearRect(0, 0, W, H);

      for (const m of motes) {
        m.ph += dt;
        m.x += (m.vx + Math.sin(m.ph) * 4) * dt;
        m.y += m.vy * dt;
        if (m.x > W + 4) m.x = -4;
        if (m.y < -4) m.y = H + 4;
        ctx.fillStyle = "rgba(235,230,217," + (m.a * (0.6 + 0.4 * Math.sin(m.ph * 2))).toFixed(3) + ")";
        ctx.beginPath();
        ctx.arc(m.x, m.y, m.r, 0, 6.283);
        ctx.fill();
      }

      wait -= dt;
      if (wait <= 0) fire();
      ctx.lineCap = "round";
      ctx.shadowColor = "#f5cc54";
      ctx.shadowBlur = 12;
      for (let i = shots.length - 1; i >= 0; i--) {
        const s = shots[i];
        s.x += s.dx * s.v * dt;
        s.y += s.dy * s.v * dt;
        if (s.x - s.len > W) { shots.splice(i, 1); continue; }
        const tx = s.x - s.dx * s.len, ty = s.y - s.dy * s.len;
        const g = ctx.createLinearGradient(tx, ty, s.x, s.y);
        g.addColorStop(0, "rgba(232,185,49,0)");
        g.addColorStop(0.8, "rgba(245,204,84,.55)");
        g.addColorStop(1, "rgba(255,244,214,.95)");
        ctx.strokeStyle = g;
        ctx.lineWidth = 2.5;
        ctx.beginPath();
        ctx.moveTo(tx, ty);
        ctx.lineTo(s.x, s.y);
        ctx.stroke();
      }
      ctx.shadowBlur = 0;
      requestAnimationFrame(frame);
    })(last);
  };
})();
