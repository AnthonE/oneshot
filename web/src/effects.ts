// Shots, blasts and projectiles: drawing only. Our own tracers come from the
// prediction the moment we fire; everyone else's from snapshots.

import * as THREE from 'three';

interface Fx {
  obj: THREE.Object3D;
  t0: number;
  life: number;
  tick: (u: number) => void;
}

export class Effects {
  private live: Fx[] = [];
  private tracerMat = new THREE.LineBasicMaterial({ color: 0xffe9a8, transparent: true });
  private blastGeo = new THREE.SphereGeometry(1, 16, 12);
  private projs: THREE.Mesh[] = [];
  private rocketGeo = new THREE.CylinderGeometry(0.06, 0.06, 0.6, 8);
  private fragGeo = new THREE.SphereGeometry(0.09, 10, 8);
  private projMat = new THREE.MeshStandardMaterial({ color: 0x444a40, emissive: 0x331100 });

  constructor(private scene: THREE.Scene) {}

  tracer(from: THREE.Vector3, to: THREE.Vector3, mine: boolean) {
    const start = mine ? from.clone().lerp(to, Math.min(1, 0.8 / Math.max(0.01, from.distanceTo(to)))) : from;
    const g = new THREE.BufferGeometry().setFromPoints([start, to]);
    const mat = this.tracerMat.clone();
    const line = new THREE.Line(g, mat);
    this.add(line, 90, (u) => (mat.opacity = 1 - u));
  }

  blast(at: THREE.Vector3, radius: number) {
    const mat = new THREE.MeshBasicMaterial({ color: 0xffa040, transparent: true });
    const m = new THREE.Mesh(this.blastGeo, mat);
    m.position.copy(at);
    this.add(m, 450, (u) => {
      m.scale.setScalar(0.3 + radius * u);
      mat.opacity = 1 - u;
    });
  }

  spark(at: THREE.Vector3) {
    const mat = new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true });
    const m = new THREE.Mesh(this.fragGeo, mat);
    m.position.copy(at);
    this.add(m, 120, (u) => {
      m.scale.setScalar(1 + u * 2);
      mat.opacity = 1 - u;
    });
  }

  /** kind, owner, x, y, z, vx, vz per projectile. */
  projectiles(p: Float32Array) {
    const n = p.length / 7;
    while (this.projs.length < n) {
      const m = new THREE.Mesh(this.fragGeo, this.projMat);
      m.castShadow = true;
      this.scene.add(m);
      this.projs.push(m);
    }
    this.projs.forEach((m, i) => {
      m.visible = i < n;
      if (i >= n) return;
      const o = i * 7;
      m.geometry = p[o] === 0 ? this.rocketGeo : this.fragGeo;
      m.position.set(p[o + 2], p[o + 3], p[o + 4]);
      if (p[o] === 0) {
        m.rotation.set(0, -Math.atan2(p[o + 6], p[o + 5]), Math.PI / 2);
      }
    });
  }

  private add(obj: THREE.Object3D, life: number, tick: (u: number) => void) {
    this.scene.add(obj);
    this.live.push({ obj, t0: performance.now(), life, tick });
  }

  update() {
    const now = performance.now();
    this.live = this.live.filter((f) => {
      const u = (now - f.t0) / f.life;
      if (u >= 1) {
        this.scene.remove(f.obj);
        return false;
      }
      f.tick(u);
      return true;
    });
  }
}
