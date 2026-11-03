// Vehicles, drawn where client-net puts them (client.vehicles()): a hull,
// a roll bar and four wheels, tilted to the ground under them. Drawing
// only: the sim's vehicle turns about y alone, and its pitch and roll here
// never touch a hitbox (sim/src/vehicle.rs).

import * as THREE from 'three';

/** sim::consts VEH_HALF_L, VEH_HALF_W, VEH_H. */
const HALF_L = 2.2, HALF_W = 1.1, H = 1.5;

/** The ground's height at (x, z) from map_terrain, as the sim has it: two
 * triangles a cell, split from the low corner to the high. 0 off it. */
export function groundAt(t: Float32Array, x: number, z: number): number {
  if (t.length <= 5) return 0;
  const [cell, x0, z0, nx, nz] = [t[0], t[1], t[2], t[3], t[4]];
  const u = (x - x0) / cell, v = (z - z0) / cell;
  if (u < 0 || v < 0 || u > nx - 1 || v > nz - 1) return 0;
  const i = Math.min(nx - 2, Math.floor(u)), j = Math.min(nz - 2, Math.floor(v));
  const fu = u - i, fv = v - j;
  const h = (a: number, b: number) => t[5 + b * nx + a];
  const [h00, h10, h01, h11] = [h(i, j), h(i + 1, j), h(i, j + 1), h(i + 1, j + 1)];
  return fu >= fv ? h00 + (h10 - h00) * fu + (h11 - h10) * fv : h00 + (h11 - h01) * fu + (h01 - h00) * fv;
}

export class Vehicles {
  private shown = new Map<number, THREE.Group>();
  private hull = new THREE.MeshStandardMaterial({ color: 0x5d6b4a, roughness: 0.6, metalness: 0.3 });
  private dark = new THREE.MeshStandardMaterial({ color: 0x23262b, roughness: 0.8 });
  private trim = new THREE.MeshStandardMaterial({ color: 0xc9a227, roughness: 0.5, metalness: 0.4 });
  terrain: Float32Array = new Float32Array(0);

  constructor(private scene: THREE.Scene) {}

  private make(): THREE.Group {
    const g = new THREE.Group();
    const add = (geo: THREE.BufferGeometry, mat: THREE.Material, x: number, y: number, z: number) => {
      const m = new THREE.Mesh(geo, mat);
      m.position.set(x, y, z);
      m.castShadow = true;
      m.receiveShadow = true;
      g.add(m);
      return m;
    };
    // The hull: a low tub, a hood, and a roll bar behind the seats.
    add(new THREE.BoxGeometry(HALF_L * 2, 0.7, HALF_W * 2), this.hull, 0, 0.75, 0);
    add(new THREE.BoxGeometry(1.2, 0.35, HALF_W * 1.8), this.hull, HALF_L - 0.7, 1.25, 0);
    add(new THREE.BoxGeometry(0.12, 0.9, HALF_W * 2), this.dark, -0.9, 1.55, 0);
    add(new THREE.BoxGeometry(0.25, 0.12, HALF_W * 2.1), this.trim, HALF_L, 0.85, 0);
    const wheel = new THREE.CylinderGeometry(0.42, 0.42, 0.35, 14);
    wheel.rotateX(Math.PI / 2);
    for (const [x, z] of [[1.5, 1.05], [1.5, -1.05], [-1.5, 1.05], [-1.5, -1.05]]) add(wheel, this.dark, x, 0.42, z);
    this.scene.add(g);
    return g;
  }

  /** `list` is client.vehicles(): slot, x y z, yaw, health, two seats. */
  update(list: Float32Array) {
    const seen = new Set<number>();
    for (let i = 0; i + 8 <= list.length; i += 8) {
      const slot = list[i], x = list[i + 1], y = list[i + 2], z = list[i + 3], yaw = list[i + 4], health = list[i + 5];
      seen.add(slot);
      let g = this.shown.get(slot);
      if (!g) this.shown.set(slot, (g = this.make()));
      // Tilted to the ground under the wheels.
      const fx = Math.cos(yaw), fz = Math.sin(yaw), rx = -fz, rz = fx;
      const at = (f: number, r: number) => groundAt(this.terrain, x + fx * f + rx * r, z + fz * f + rz * r);
      const pitch = Math.atan2(at(HALF_L, 0) - at(-HALF_L, 0), HALF_L * 2);
      const roll = Math.atan2(at(0, HALF_W) - at(0, -HALF_W), HALF_W * 2);
      g.position.set(x, y, z);
      g.rotation.set(0, 0, 0);
      g.rotateY(-yaw);
      g.rotateZ(pitch);
      g.rotateX(-roll);
      // Scorched as it's hurt.
      (g.children[0] as THREE.Mesh).material = health < 0.3 ? this.dark : this.hull;
    }
    for (const [slot, g] of this.shown) {
      if (!seen.has(slot)) {
        g.removeFromParent();
        this.shown.delete(slot);
      }
    }
  }

  clear() {
    for (const g of this.shown.values()) g.removeFromParent();
    this.shown.clear();
  }
}

/** sim::consts::VEH_H, for the chase camera. */
export const VEHICLE_H = H;
