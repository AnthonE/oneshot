// The level: the sim's own boxes (map_boxes), lit and shadowed. The
// geometry comes from the sim, so what you see is what you collide with and
// what stops bullets.

import * as THREE from 'three';

function gridTexture(base: string, line: string, cells: number): THREE.CanvasTexture {
  const c = document.createElement('canvas');
  c.width = c.height = 256;
  const g = c.getContext('2d')!;
  g.fillStyle = base;
  g.fillRect(0, 0, 256, 256);
  g.strokeStyle = line;
  g.lineWidth = 2;
  const step = 256 / cells;
  for (let i = 0; i <= cells; i++) {
    g.beginPath();
    g.moveTo(i * step, 0);
    g.lineTo(i * step, 256);
    g.moveTo(0, i * step);
    g.lineTo(256, i * step);
    g.stroke();
  }
  const t = new THREE.CanvasTexture(c);
  t.wrapS = t.wrapT = THREE.RepeatWrapping;
  t.colorSpace = THREE.SRGBColorSpace;
  t.anisotropy = 8;
  return t;
}

export interface Materials {
  floor: THREE.Material;
  wall: THREE.Material;
  block: THREE.Material;
  trim: THREE.Material;
}

export function defaultMaterials(): Materials {
  const floor = gridTexture('#5d6168', '#4b4f55', 4);
  floor.repeat.set(24, 24);
  return {
    floor: new THREE.MeshStandardMaterial({ map: floor, roughness: 0.9 }),
    wall: new THREE.MeshStandardMaterial({ color: 0x8a8f96, roughness: 0.85 }),
    block: new THREE.MeshStandardMaterial({ color: 0x9da3ab, roughness: 0.7, metalness: 0.1 }),
    trim: new THREE.MeshStandardMaterial({ color: 0xc9a227, roughness: 0.5, metalness: 0.4 }),
  };
}

export function buildWorld(scene: THREE.Scene, boxes: Float32Array, pads: Float32Array, mats: Materials) {
  scene.background = new THREE.Color(0x9fc4e8);
  scene.fog = new THREE.Fog(0x9fc4e8, 60, 160);

  const hemi = new THREE.HemisphereLight(0xdfefff, 0x4a4036, 1.1);
  scene.add(hemi);
  const sun = new THREE.DirectionalLight(0xfff2dd, 2.2);
  sun.position.set(-30, 50, 20);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  const sc = sun.shadow.camera as THREE.OrthographicCamera;
  sc.left = sc.bottom = -32;
  sc.right = sc.top = 32;
  sc.near = 1;
  sc.far = 120;
  sun.shadow.bias = -0.0005;
  scene.add(sun);

  const floor = new THREE.Mesh(new THREE.PlaneGeometry(100, 100), mats.floor);
  floor.rotation.x = -Math.PI / 2;
  floor.receiveShadow = true;
  scene.add(floor);

  for (let i = 0; i < boxes.length; i += 6) {
    const [x0, y0, z0, x1, y1, z1] = boxes.subarray(i, i + 6);
    const w = x1 - x0, h = y1 - y0, d = z1 - z0;
    const tall = h >= 6;
    const m = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), tall ? mats.wall : mats.block);
    m.position.set((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2);
    m.castShadow = !tall;
    m.receiveShadow = true;
    scene.add(m);
  }

  const padMeshes: THREE.Mesh[] = [];
  for (let i = 0; i < pads.length; i += 5) {
    const ring = new THREE.Mesh(new THREE.CylinderGeometry(0.8, 0.8, 0.06, 24), mats.trim);
    ring.position.set(pads[i], pads[i + 1] + 0.03, pads[i + 2]);
    ring.receiveShadow = true;
    scene.add(ring);
    padMeshes.push(ring);
  }
  return { sun, padMeshes };
}
