// The HUD: plain DOM over the canvas. Reticle, shields and health, ammo,
// frags, score and clock, kill feed, hit marker, damage direction, the net
// graph on N.

const css = `
#hud { position: fixed; inset: 0; pointer-events: none; font: 600 15px system-ui, sans-serif; color: #fff;
  text-shadow: 0 1px 2px #000a; user-select: none; }
#hud .reticle { position: absolute; left: 50%; top: 50%; width: 22px; height: 22px; margin: -11px 0 0 -11px;
  border: 2px solid #fffc; border-radius: 50%; }
#hud .reticle::after { content: ''; position: absolute; left: 50%; top: 50%; width: 3px; height: 3px; margin: -1.5px;
  background: #fff; border-radius: 50%; }
#hud .hit { position: absolute; left: 50%; top: 50%; width: 34px; height: 34px; margin: -17px; opacity: 0;
  background: linear-gradient(45deg, transparent 46%, #fff 46%, #fff 54%, transparent 54%),
              linear-gradient(-45deg, transparent 46%, #fff 46%, #fff 54%, transparent 54%); }
#hud .hit.kill { filter: drop-shadow(0 0 2px #f44) hue-rotate(-30deg); }
#hud .bars { position: absolute; left: 50%; top: 18px; transform: translateX(-50%); width: min(420px, 60vw); }
#hud .bar { height: 10px; background: #0006; border: 1px solid #fff6; margin-bottom: 4px; border-radius: 2px; overflow: hidden; }
#hud .bar > i { display: block; height: 100%; background: #4cc3ff; }
#hud .bar.health > i { background: #f5f5f5; }
#hud .ammo { position: absolute; right: 24px; bottom: 20px; text-align: right; }
#hud .ammo b { font-size: 34px; }
#hud .score { position: absolute; left: 24px; bottom: 20px; }
#hud .score div { margin-top: 2px; }
#hud .feed { position: absolute; right: 24px; top: 18px; text-align: right; }
#hud .center { position: absolute; left: 0; right: 0; top: 34%; text-align: center; font-size: 28px; white-space: pre-line; }
#hud .net { position: absolute; left: 12px; top: 12px; font: 12px ui-monospace, monospace; background: #0008; padding: 6px 8px;
  white-space: pre; display: none; }
#hud .dmg { position: absolute; left: 50%; top: 50%; width: 220px; height: 220px; margin: -110px; opacity: 0;
  border-radius: 50%; border-top: 6px solid #f33c; }
#hud.killcam { box-shadow: inset 0 0 0 4px #e8b93166; }
#hud.killcam .bars, #hud.killcam .ammo { opacity: 0.25; }
#hud .scope { position: absolute; inset: 0; display: none;
  background: radial-gradient(circle at center, transparent 0, transparent 34vh, #000 34.2vh); }
`;

export class Hud {
  root = document.createElement('div');
  private el: Record<string, HTMLElement> = {};
  private hitT = 0;
  private dmgT = 0;
  private feed: { text: string; t: number }[] = [];
  names: string[] = [];
  netOn = false;

  constructor() {
    const style = document.createElement('style');
    style.textContent = css;
    document.head.appendChild(style);
    this.root.id = 'hud';
    this.root.innerHTML = `
      <div class="scope"></div>
      <div class="reticle"></div><div class="hit"></div><div class="dmg"></div>
      <div class="bars"><div class="bar shield"><i></i></div><div class="bar health"><i></i></div></div>
      <div class="ammo"></div><div class="score"></div><div class="feed"></div>
      <div class="center"></div><div class="net"></div>`;
    for (const k of ['scope', 'reticle', 'hit', 'dmg', 'ammo', 'score', 'feed', 'center', 'net']) {
      this.el[k] = this.root.querySelector('.' + k)!;
    }
    this.el.shield = this.root.querySelector('.shield > i')!;
    this.el.health = this.root.querySelector('.health > i')!;
    document.body.appendChild(this.root);
  }

  name(slot: number): string {
    return this.names[slot] || `P${slot + 1}`;
  }

  hit(kill: boolean) {
    this.hitT = performance.now();
    this.el.hit.classList.toggle('kill', kill);
  }

  damage(angle: number) {
    this.dmgT = performance.now();
    this.el.dmg.style.transform = `rotate(${angle}rad)`;
  }

  kill(text: string) {
    this.feed.push({ text, t: performance.now() });
    if (this.feed.length > 5) this.feed.shift();
  }

  update(o: {
    shield: number; health: number; ammo: string; reserve: string; grenades: number; weapon: string; other: string;
    score: string[]; center: string; net: string; zoom: boolean; reloading: number; killcam?: boolean;
  }) {
    const now = performance.now();
    this.el.shield.style.width = `${Math.max(0, Math.min(100, o.shield))}%`;
    this.el.health.style.width = `${Math.max(0, Math.min(100, o.health))}%`;
    this.el.ammo.innerHTML = `${o.weapon}${o.reloading > 0 ? ' · reloading' : ''}<br><b>${o.ammo}</b> / ${o.reserve}` +
      `<br>${'●'.repeat(o.grenades)}<br><small>${o.other}</small>`;
    this.el.score.innerHTML = o.score.map((s) => `<div>${s}</div>`).join('');
    this.feed = this.feed.filter((f) => now - f.t < 5000);
    this.el.feed.innerHTML = this.feed.map((f) => `<div>${f.text}</div>`).join('');
    this.el.center.textContent = o.center;
    this.el.hit.style.opacity = String(Math.max(0, 1 - (now - this.hitT) / 220));
    this.el.dmg.style.opacity = String(Math.max(0, 1 - (now - this.dmgT) / 600));
    this.el.net.style.display = this.netOn ? 'block' : 'none';
    this.el.net.textContent = o.net;
    this.el.scope.style.display = o.zoom ? 'block' : 'none';
    this.el.reticle.style.display = o.zoom || o.killcam ? 'none' : 'block';
    this.root.classList.toggle('killcam', !!o.killcam);
  }
}
