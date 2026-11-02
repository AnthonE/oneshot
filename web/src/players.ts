// Everyone else, drawn where client-net interpolated them. A body per
// seat; `Players.setModel` swaps the stand-in capsule for a skinned
// character once one has loaded.

import * as THREE from 'three';

export const STRIDE = 14; // client.others(): see client-net's web.rs

export const TEAM_COLORS = [0xd8453b, 0x3b7bd8];

/** Each skin's trim (proto::SKINS order), drawn over the side's colour and
 * never instead of it: a skin changes no hitbox and no fighter's
 * visibility (docs/DESIGN.md §15). Null: the standard look. */
export const SKIN_TRIM: (number | null)[] = [null, 0xf2f2f2, 0xff6a1a, 0x8fd8ff, 0x58d06a, 0xe8b931];

export interface Body {
  root: THREE.Object3D;
  update(o: Float32Array, i: number, dt: number): void;
  dispose(): void;
}

export type BodyFactory = (team: number, skin: number) => Body;

/** The stand-in: a capsule and a visor (in the skin's trim). */
export function capsuleBody(team: number, skin = 0): Body {
  const root = new THREE.Group();
  const mat = new THREE.MeshStandardMaterial({ color: TEAM_COLORS[team & 1], roughness: 0.6 });
  const body = new THREE.Mesh(new THREE.CapsuleGeometry(0.35, 1.1, 6, 12), mat);
  body.position.y = 0.9;
  body.castShadow = true;
  root.add(body);
  const visor = new THREE.Mesh(
    new THREE.BoxGeometry(0.12, 0.12, 0.36),
    new THREE.MeshStandardMaterial({ color: SKIN_TRIM[skin] ?? 0xffcc33, emissive: 0x442200 }),
  );
  visor.position.set(0.3, 1.55, 0);
  root.add(visor);
  return {
    root,
    update(o, i) {
      const crouched = o[i + 3] > 0;
      body.scale.y = crouched ? 0.72 : 1;
      body.position.y = crouched ? 0.65 : 0.9;
      visor.position.y = crouched ? 1.08 : 1.55;
    },
    dispose() {
      mat.dispose();
    },
  };
}

export class Players {
  private bodies = new Map<number, { body: Body; team: number; skin: number }>();
  private factory: BodyFactory = capsuleBody;
  /** Each seat's skin, from the server's seat messages. */
  skins: number[] = [];

  constructor(private scene: THREE.Scene) {}

  setModel(f: BodyFactory) {
    this.factory = f;
    for (const [slot, b] of this.bodies) {
      this.scene.remove(b.body.root);
      b.body.dispose();
      this.bodies.delete(slot);
    }
  }

  /** `o` is client.others(now). */
  update(o: Float32Array, dt: number) {
    const seen = new Set<number>();
    for (let i = 0; i + STRIDE <= o.length; i += STRIDE) {
      const slot = o[i], team = o[i + 1];
      seen.add(slot);
      const skin = this.skins[slot] ?? 0;
      let b = this.bodies.get(slot);
      if (!b || b.team !== team || b.skin !== skin) {
        if (b) {
          this.scene.remove(b.body.root);
          b.body.dispose();
        }
        b = { body: this.factory(team, skin), team, skin };
        this.scene.add(b.body.root);
        this.bodies.set(slot, b);
      }
      const r = b.body.root;
      r.visible = o[i + 2] > 0 || true;
      r.position.set(o[i + 6], o[i + 7], o[i + 8]);
      // Sim yaw 0 faces +x and grows toward +z; three's rotation.y grows
      // the other way round.
      r.rotation.y = -o[i + 9];
      b.body.update(o, i, dt);
    }
    for (const [slot, b] of this.bodies) {
      if (!seen.has(slot)) {
        this.scene.remove(b.body.root);
        b.body.dispose();
        this.bodies.delete(slot);
      }
    }
  }
}
