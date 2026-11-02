// Models, the gun in view and sound, loaded after the level so the game is
// playable while they come: Triple T for everyone else, Gates' guns in hand
// and in view, Gates' CC0 sounds. All of it is drawing; nothing here is fed
// back.

import * as THREE from 'three';
import { GLTFLoader, type GLTF } from 'three/addons/loaders/GLTFLoader.js';
import { KTX2Loader } from 'three/addons/loaders/KTX2Loader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';
import { type Body, type Players, TEAM_COLORS } from './players';
import { FIRE, type Frame } from './input';

export interface Art {
  frame(own: Float32Array, f: Frame, dt: number, now: number, camera: THREE.PerspectiveCamera): void;
  afterRender(renderer: THREE.WebGLRenderer): void;
  shot(mine: boolean, arm: number, at: THREE.Vector3): void;
  hit(mine: boolean, part: number, onMe: boolean): void;
  blast(at: THREE.Vector3): void;
}

// The guns (arm ids from sim::arms) and how each is held: Gates' grip
// numbers (crates/client/src/ui/hold.rs). Each model stands barrel up (+Y),
// top toward +Z, stock at y = 0.
interface GunDef {
  file: string;
  height: number;
  grip: number;
  scale: number;
  long: boolean;
  sound: string;
  takes: number;
  far: string;
  reload: string;
}
const GUNS: Record<number, GunDef> = {
  1: { file: 'ar', height: 0.888, grip: 0.32, scale: 0.85, long: true, sound: 'shot_rifle', takes: 3, far: 'shot_rifle_far', reload: 'reload_rifle_0' },
  2: { file: 'br', height: 0.957, grip: 0.36, scale: 0.85, long: true, sound: 'shot_heli', takes: 3, far: 'shot_heli_far', reload: 'reload_rifle_0' },
  3: { file: 'magnum', height: 0.325, grip: 0.17, scale: 1.0, long: false, sound: 'shot_gun', takes: 2, far: 'shot_gun_far', reload: 'reload_pistol_0' },
  4: { file: 'sniper', height: 1.09, grip: 0.33, scale: 0.8, long: true, sound: 'shot_bolt', takes: 3, far: 'shot_bolt_far', reload: 'reload_bolt_0' },
  5: { file: 'shotgun', height: 0.949, grip: 0.3, scale: 0.85, long: true, sound: 'shot_shotgun', takes: 3, far: 'shot_shotgun_far', reload: 'reload_shotgun_0' },
  6: { file: 'rocket-launcher', height: 1.058, grip: 0.45, scale: 0.8, long: true, sound: 'shot_rocket', takes: 3, far: 'shot_rifle_far', reload: 'reload_rifle_0' },
  7: { file: 'smg', height: 0.621, grip: 0.42, scale: 0.9, long: true, sound: 'shot_smg', takes: 3, far: 'shot_smg_far', reload: 'reload_rifle_0' },
};

/** Triple T faces +Z in its file; our bodies face +X. */
const FACE = Math.PI / 2;

// Model axes onto a holder's: barrel (+Y) along the holder's forward, top
// (+Z) up. Third person forward is +X, the camera's is −Z.
const BODY_BASIS = new THREE.Quaternion().setFromRotationMatrix(
  new THREE.Matrix4().makeBasis(new THREE.Vector3(0, 0, 1), new THREE.Vector3(1, 0, 0), new THREE.Vector3(0, 1, 0)),
);
const VIEW_BASIS = new THREE.Quaternion().setFromRotationMatrix(
  new THREE.Matrix4().makeBasis(new THREE.Vector3(1, 0, 0), new THREE.Vector3(0, 0, -1), new THREE.Vector3(0, 1, 0)),
);

/** A gun posed so its grip sits at the returned group's origin. */
function posedGun(src: THREE.Object3D, d: GunDef, basis: THREE.Quaternion): THREE.Group {
  const pivot = new THREE.Group();
  pivot.quaternion.copy(basis);
  const scaler = new THREE.Group();
  scaler.scale.setScalar(d.scale);
  const m = src.clone(true);
  m.position.y = -d.height * d.grip;
  m.traverse((o) => {
    if ((o as THREE.Mesh).isMesh) o.castShadow = true;
  });
  scaler.add(m);
  pivot.add(scaler);
  const muzzle = new THREE.Object3D();
  muzzle.position.y = d.height * (1 - d.grip);
  scaler.add(muzzle);
  pivot.userData.muzzle = muzzle;
  return pivot;
}

// ------------------------------------------------------------------ sound

class Sfx {
  ctx: AudioContext | null = null;
  private bufs = new Map<string, AudioBuffer>();
  private master!: GainNode;

  async load(names: string[]) {
    try {
      this.ctx = new AudioContext();
    } catch {
      return;
    }
    this.master = this.ctx.createGain();
    this.master.gain.value = 0.6;
    this.master.connect(this.ctx.destination);
    const resume = () => this.ctx?.resume();
    addEventListener('pointerdown', resume);
    addEventListener('keydown', resume);
    await Promise.all(
      names.map(async (n) => {
        try {
          const r = await fetch(`sound/${n}.ogg`);
          if (r.ok) this.bufs.set(n, await this.ctx!.decodeAudioData(await r.arrayBuffer()));
        } catch {
          /* a missing sound is silence */
        }
      }),
    );
  }

  listen(cam: THREE.Camera) {
    const l = this.ctx?.listener;
    if (!l || !l.positionX) return;
    const p = cam.getWorldPosition(new THREE.Vector3());
    const f = cam.getWorldDirection(new THREE.Vector3());
    l.positionX.value = p.x;
    l.positionY.value = p.y;
    l.positionZ.value = p.z;
    l.forwardX.value = f.x;
    l.forwardY.value = f.y;
    l.forwardZ.value = f.z;
    l.upX.value = 0;
    l.upY.value = 1;
    l.upZ.value = 0;
  }

  play(name: string, o: { at?: THREE.Vector3; gain?: number; rate?: number } = {}) {
    const ctx = this.ctx, buf = this.bufs.get(name);
    if (!ctx || !buf || ctx.state !== 'running') return;
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.playbackRate.value = (o.rate ?? 1) * (0.96 + Math.random() * 0.08);
    const g = ctx.createGain();
    g.gain.value = o.gain ?? 1;
    src.connect(g);
    if (o.at) {
      const p = ctx.createPanner();
      p.panningModel = 'HRTF';
      p.distanceModel = 'inverse';
      p.refDistance = 4;
      p.rolloffFactor = 1.1;
      p.positionX.value = o.at.x;
      p.positionY.value = o.at.y;
      p.positionZ.value = o.at.z;
      g.connect(p).connect(this.master);
    } else {
      g.connect(this.master);
    }
    src.start();
  }
}

// ------------------------------------------------------------- the bodies

const CLIPS = ['Idle_Loop', 'Walk_Loop', 'Jog_Fwd_Loop', 'Sprint_Loop', 'Crouch_Idle_Loop', 'Crouch_Fwd_Loop',
  'Jump_Loop', 'Death01', 'Rifle_Hold_Loop', 'Pistol_Idle_Loop'];

function bodyFactory(base: GLTF, guns: Map<number, THREE.Object3D>) {
  const clips = new Map(base.animations.map((a) => [a.name, a]));
  return (team: number): Body => {
    const root = new THREE.Group();
    const model = SkeletonUtils.clone(base.scene);
    model.rotation.y = FACE;
    const tint = new THREE.Color(TEAM_COLORS[team & 1]);
    model.traverse((o) => {
      const m = o as THREE.SkinnedMesh;
      if (!m.isMesh) return;
      m.castShadow = true;
      m.frustumCulled = false;
      const mat = (m.material as THREE.MeshStandardMaterial).clone();
      // The albedo is very dark (Gates' MANIFEST: luma 0.056): lift it,
      // and tint it by side so a fighter reads at a glance.
      mat.color = new THREE.Color(2.6, 2.6, 2.6).lerp(tint.clone().multiplyScalar(3), 0.35);
      mat.emissive = tint.clone().multiplyScalar(0.08);
      m.material = mat;
    });
    root.add(model);
    const mixer = new THREE.AnimationMixer(model);
    const actions = new Map<string, THREE.AnimationAction>();
    for (const n of CLIPS) {
      const c = clips.get(n);
      if (c) actions.set(n, mixer.clipAction(c));
    }
    let current = '';
    const play = (n: string) => {
      if (n === current) return;
      const next = actions.get(n);
      if (!next) return;
      const prev = actions.get(current);
      next.reset();
      if (n === 'Death01') {
        next.setLoop(THREE.LoopOnce, 1);
        next.clampWhenFinished = true;
      }
      next.play();
      if (prev) next.crossFadeFrom(prev, 0.15, false);
      current = n;
    };
    const hand = new THREE.Group();
    hand.position.set(0.18, 1.3, 0.2);
    root.add(hand);
    let held = -1;
    const posed = new Map<number, THREE.Group>();
    return {
      root,
      update(o, i, dt) {
        const alive = o[i + 2] > 0, crouched = o[i + 3] > 0, grounded = o[i + 4] > 0;
        const pitch = o[i + 10], arm = o[i + 11], speed = o[i + 13];
        if (!alive) play('Death01');
        else if (!grounded) play('Jump_Loop');
        else if (crouched) play(speed > 0.5 ? 'Crouch_Fwd_Loop' : 'Crouch_Idle_Loop');
        else if (speed > 6.5) play('Sprint_Loop');
        else if (speed > 3.5) play('Jog_Fwd_Loop');
        else if (speed > 0.4) play('Walk_Loop');
        else play(GUNS[arm]?.long === false ? 'Pistol_Idle_Loop' : 'Rifle_Hold_Loop');
        mixer.update(dt);
        if (arm !== held) {
          hand.clear();
          held = arm;
          const d = GUNS[arm], src = guns.get(arm);
          if (d && src) {
            let g = posed.get(arm);
            if (!g) posed.set(arm, (g = posedGun(src, d, BODY_BASIS)));
            hand.add(g);
          }
        }
        hand.visible = alive;
        hand.position.y = crouched ? 0.9 : 1.3;
        hand.rotation.z = pitch;
      },
      dispose() {
        mixer.stopAllAction();
      },
    };
  };
}

// ----------------------------------------------------------- the viewmodel

class Viewmodel {
  scene = new THREE.Scene();
  cam = new THREE.PerspectiveCamera(70, 1, 0.01, 10);
  private hold = new THREE.Group();
  private gun: THREE.Group | null = null;
  private arm = -1;
  private kick = 0;
  private bob = 0;
  private flash: THREE.PointLight;
  private flashT = 0;
  private posed = new Map<number, THREE.Group>();

  constructor(private guns: Map<number, THREE.Object3D>) {
    this.scene.add(new THREE.HemisphereLight(0xdfefff, 0x4a4036, 1.4));
    const sun = new THREE.DirectionalLight(0xfff2dd, 1.8);
    sun.position.set(-1, 2, 1);
    this.scene.add(sun);
    this.flash = new THREE.PointLight(0xffc066, 0, 3);
    this.cam.add(this.flash);
    this.flash.position.set(0.3, -0.2, -1);
    this.scene.add(this.cam);
    // Gates' VIEWMODEL_HOLD: where the item sits in view space.
    this.hold.position.set(0.3, -0.29, -0.5);
    this.cam.add(this.hold);
  }

  fired() {
    this.kick = 1;
    this.flashT = 1;
  }

  update(arm: number, alive: boolean, zoomed: boolean, reloading: number, speed: number, dt: number, aspect: number) {
    this.cam.aspect = aspect;
    this.cam.updateProjectionMatrix();
    if (arm !== this.arm) {
      this.arm = arm;
      this.hold.clear();
      const d = GUNS[arm], src = this.guns.get(arm);
      this.gun = null;
      if (d && src) {
        let g = this.posed.get(arm);
        if (!g) this.posed.set(arm, (g = posedGun(src, d, VIEW_BASIS)));
        this.gun = g;
        this.hold.add(g);
      }
    }
    this.hold.visible = alive && !zoomed;
    this.kick = Math.max(0, this.kick - dt * 9);
    this.flashT = Math.max(0, this.flashT - dt * 25);
    this.bob += dt * speed * 1.6;
    const sway = Math.min(1, speed / 7);
    const dip = reloading > 0 ? Math.sin(Math.min(1, reloading) * Math.PI) : 0;
    this.hold.position.set(
      0.3 + Math.sin(this.bob) * 0.012 * sway,
      -0.29 + Math.abs(Math.cos(this.bob)) * 0.012 * sway - dip * 0.12,
      -0.5 + this.kick * 0.06,
    );
    this.hold.rotation.set(this.kick * 0.09 - dip * 0.5, 0.06, dip * 0.4);
    this.flash.intensity = this.flashT * 4;
  }
}

// --------------------------------------------------------------- loading

export async function loadArt(
  scene: THREE.Scene,
  camera: THREE.PerspectiveCamera,
  renderer: THREE.WebGLRenderer,
  players: Players,
): Promise<Art> {
  const ktx2 = new KTX2Loader().setTranscoderPath('basis/').detectSupport(renderer);
  const loader = new GLTFLoader().setKTX2Loader(ktx2);
  const load = (f: string) => loader.loadAsync(`models/${f}.glb`);
  const sfx = new Sfx();
  const names = new Set<string>(['dry_fire_0', 'headshot_0', 'headshot_1', 'hit_0', 'equip_0', 'blast_0', 'blast_1',
    'flesh_0', 'flesh_1', 'hurt_0', 'death_0', 'swing_0', 'land_0', 'step_concrete_0', 'step_concrete_1',
    'step_concrete_2', 'step_concrete_3']);
  for (const d of Object.values(GUNS)) {
    for (let k = 0; k < d.takes; k++) names.add(`${d.sound}_${k}`);
    names.add(`${d.far}_0`);
    names.add(d.reload);
  }
  const [tt, ...gunScenes] = await Promise.all([
    load('triple-t'),
    ...Object.values(GUNS).map((d) => load(d.file).then((g) => g.scene)),
    sfx.load([...names]),
  ]);
  const guns = new Map<number, THREE.Object3D>();
  Object.keys(GUNS).forEach((k, i) => guns.set(Number(k), gunScenes[i] as THREE.Object3D));
  players.setModel(bodyFactory(tt as GLTF, guns));

  const view = new Viewmodel(guns);
  const muzzleLight = new THREE.PointLight(0xffc066, 0, 6);
  scene.add(muzzleLight);
  let lastReload = 0, lastGrounded = true, stepT = 0, dryT = 0, lastArm = 0, lastMag = -1;
  const v = new THREE.Vector3();

  return {
    frame(own, f, dt, _now, cam) {
      const alive = own[6] > 0, arm = own[10], mag = own[11], reloading = own[15];
      const grounded = own[20] > 0, speed = own[21];
      const zoomed = (f.buttons & (1 << 8)) !== 0 && (arm === 4 || arm === 2 || arm === 3);
      view.update(arm, alive, zoomed, reloading, grounded ? speed : 0, dt, cam.aspect);
      sfx.listen(cam);
      muzzleLight.intensity = Math.max(0, muzzleLight.intensity - dt * 120);
      if (reloading > 0 && lastReload === 0) sfx.play(GUNS[arm]?.reload ?? 'reload_rifle_0', { gain: 0.7 });
      lastReload = reloading;
      if (arm !== lastArm) sfx.play('equip_0', { gain: 0.5 });
      lastArm = arm;
      dryT -= dt;
      if (alive && mag === 0 && lastMag === 0 && (f.buttons & FIRE) && dryT <= 0) {
        sfx.play('dry_fire_0', { gain: 0.6 });
        dryT = 0.3;
      }
      lastMag = mag;
      if (alive && grounded && speed > 2.5) {
        stepT -= dt * speed;
        if (stepT <= 0) {
          sfx.play(`step_concrete_${(Math.random() * 4) | 0}`, { gain: 0.25 });
          stepT = 2.6;
        }
      }
      if (grounded && !lastGrounded) sfx.play('land_0', { gain: 0.35 });
      lastGrounded = grounded;
      void camera;
    },
    afterRender(r) {
      const ac = r.autoClear;
      r.autoClear = false;
      r.clearDepth();
      r.render(view.scene, view.cam);
      r.autoClear = ac;
    },
    shot(mine, arm, at) {
      const d = GUNS[arm];
      if (!d) return;
      if (mine) {
        view.fired();
        sfx.play(`${d.sound}_${(Math.random() * d.takes) | 0}`, { gain: 0.55 });
        muzzleLight.position.copy(at);
        muzzleLight.intensity = 3;
      } else {
        const far = v.copy(at).distanceTo(camera.position) > 35;
        sfx.play(far ? `${d.far}_0` : `${d.sound}_${(Math.random() * d.takes) | 0}`, { at, gain: 0.9 });
      }
    },
    hit(mine, part, onMe) {
      if (mine) sfx.play(part === 2 ? `headshot_${(Math.random() * 2) | 0}` : 'hit_0', { gain: 0.5 });
      if (onMe) sfx.play(`flesh_${(Math.random() * 2) | 0}`, { gain: 0.6 });
    },
    blast(at) {
      sfx.play(`blast_${(Math.random() * 2) | 0}`, { at, gain: 1.2 });
    },
  };
}
