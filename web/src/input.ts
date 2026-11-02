// The controls: pointer lock and raw mouse for aim, keys for the rest
// (bindings after Fireplace's arena). Taps are latched until the next frame
// reads them, so a press shorter than a frame still counts.

export const FIRE = 1 << 0;
export const JUMP = 1 << 1;
export const CROUCH = 1 << 2;
export const MELEE = 1 << 3;
export const GRENADE = 1 << 4;
export const RELOAD = 1 << 5;
export const SWAP = 1 << 6;
export const USE = 1 << 7;
export const ZOOM = 1 << 8;

const KEYS: Record<string, number> = {
  Space: JUMP,
  ControlLeft: CROUCH,
  KeyC: CROUCH,
  KeyQ: MELEE,
  KeyV: MELEE,
  KeyG: GRENADE,
  KeyR: RELOAD,
  KeyX: SWAP,
  Digit1: SWAP,
  Digit2: SWAP,
  KeyE: USE,
};

const TURN = 65536;
const PITCH_MAX = (89 / 360) * TURN;

export interface Frame {
  buttons: number;
  yaw: number; // u16
  pitch: number; // i16
  mx: number;
  mz: number;
}

export class Controls {
  /** Radians: yaw 0 faces +x and grows toward +z; pitch up is positive. */
  yaw = 0;
  pitch = 0;
  sens = 0.0022;
  locked = false;
  private held = new Set<string>();
  private mouse = 0;
  private latched = 0;
  private ac = new AbortController();

  constructor(private target: HTMLElement) {
    const on = (el: EventTarget, ev: string, f: (e: any) => void) =>
      el.addEventListener(ev, f, { signal: this.ac.signal });
    on(target, 'click', () => {
      if (!this.locked) {
        (target.requestPointerLock as any)({ unadjustedMovement: true })?.catch?.(() => target.requestPointerLock());
      }
    });
    on(document, 'pointerlockchange', () => {
      this.locked = document.pointerLockElement === target;
      if (!this.locked) {
        this.held.clear();
        this.mouse = 0;
      }
    });
    on(document, 'mousemove', (e: MouseEvent) => {
      if (!this.locked) return;
      const s = this.sens * (this.mouse & ZOOM ? 0.45 : 1);
      this.yaw += e.movementX * s;
      this.pitch = Math.max(-1.55, Math.min(1.55, this.pitch - e.movementY * s));
    });
    on(document, 'mousedown', (e: MouseEvent) => {
      if (!this.locked) return;
      if (e.button === 0) this.mouse |= FIRE;
      if (e.button === 2) this.mouse |= ZOOM;
    });
    on(document, 'mouseup', (e: MouseEvent) => {
      if (e.button === 0) this.mouse &= ~FIRE;
      if (e.button === 2) this.mouse &= ~ZOOM;
    });
    on(document, 'contextmenu', (e: Event) => e.preventDefault());
    on(document, 'wheel', () => {
      if (this.locked) this.latched |= SWAP;
    });
    on(document, 'keydown', (e: KeyboardEvent) => {
      if (!this.locked) return;
      if (e.code === 'Tab') e.preventDefault();
      this.held.add(e.code);
      this.latched |= KEYS[e.code] ?? 0;
    });
    on(document, 'keyup', (e: KeyboardEvent) => this.held.delete(e.code));
    on(window, 'blur', () => {
      this.held.clear();
      this.mouse = 0;
    });
  }

  down(code: string): boolean {
    return this.held.has(code);
  }

  /** The controls as they are now, as the wire wants them. */
  frame(): Frame {
    let buttons = this.mouse | this.latched;
    this.latched = 0;
    for (const k of this.held) buttons |= KEYS[k] ?? 0;
    const ax = (this.held.has('KeyD') ? 1 : 0) - (this.held.has('KeyA') ? 1 : 0);
    const az = (this.held.has('KeyW') ? 1 : 0) - (this.held.has('KeyS') ? 1 : 0);
    return {
      buttons,
      yaw: (((Math.round((this.yaw / (2 * Math.PI)) * TURN) % TURN) + TURN) % TURN),
      pitch: Math.max(-PITCH_MAX, Math.min(PITCH_MAX, Math.round((this.pitch / (2 * Math.PI)) * TURN))),
      mx: ax * 127,
      mz: az * 127,
    };
  }

  dispose() {
    this.ac.abort();
    if (document.pointerLockElement === this.target) document.exitPointerLock();
  }
}
