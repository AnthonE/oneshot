// The level: the sim's own boxes (map_boxes) and ground (map_terrain), lit
// and shadowed, and its launchers. The geometry comes from the sim, so what
// you see is what you collide with and what stops bullets.

import * as THREE from 'three';

export function gridTexture(base: string, line: string, cells: number): THREE.CanvasTexture {
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
  red: THREE.Material;
  blue: THREE.Material;
  dark: THREE.Material;
  rock: THREE.Material;
  ground: THREE.Material;
}

/** A CC0 texture pair from Gates (tools/assets.mjs), tiled per metre by
 * the geometry's UVs (tileUVs). Until it loads, or if it never does, the
 * material's own colour shows. */
function textured(name: string, color: number, roughness: number, metalness = 0): THREE.MeshStandardMaterial {
  const mat = new THREE.MeshStandardMaterial({ color, roughness, metalness });
  const tl = new THREE.TextureLoader();
  const wrap = (t: THREE.Texture) => {
    t.wrapS = t.wrapT = THREE.RepeatWrapping;
    t.anisotropy = 8;
    return t;
  };
  tl.load(`textures/${name}_albedo.jpg`, (t) => {
    t.colorSpace = THREE.SRGBColorSpace;
    mat.map = wrap(t);
    mat.color.set(0xffffff);
    mat.needsUpdate = true;
  }, undefined, () => {});
  tl.load(`textures/${name}_normal.jpg`, (t) => {
    mat.normalMap = wrap(t);
    mat.needsUpdate = true;
  }, undefined, () => {});
  return mat;
}

export function defaultMaterials(): Materials {
  return {
    floor: textured('aggregate', 0x6a6e74, 0.95),
    wall: textured('concrete', 0x8a8f96, 0.9),
    block: textured('metal', 0x9da3ab, 0.6, 0.3),
    trim: new THREE.MeshStandardMaterial({ color: 0xc9a227, roughness: 0.5, metalness: 0.4 }),
    // The sides' colours on their own ground: muted, so a fighter in team
    // colours still stands out against them.
    red: textured('metal', 0x8a5450, 0.6, 0.3),
    blue: textured('metal', 0x50648a, 0.6, 0.3),
    dark: textured('metal', 0x4a4f57, 0.7, 0.3),
    // Canyon rock and its ground: drawn in the colours the terrain mesh
    // carries (vertex colours: sand low, grass on the flats, rock on the
    // slopes).
    rock: new THREE.MeshStandardMaterial({ color: 0x9c5536, roughness: 0.95 }),
    ground: new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.95 }),
  };
}

/** Box looks, as maps/<name>/map.json names them (tools/maps/compile.mjs LOOKS). */
const LOOKS: (keyof Materials)[] = ['wall', 'block', 'trim', 'red', 'blue', 'dark', 'rock'];

/** Scales a box's UVs to its size so a texture tiles every `tile` metres
 * on every face. BoxGeometry's faces come in order +x −x +y −y +z −z, four
 * vertices each. */
function tileUVs(g: THREE.BoxGeometry, w: number, h: number, d: number, tile: number) {
  const uv = g.attributes.uv as THREE.BufferAttribute;
  const dims = [[d, h], [d, h], [w, d], [w, d], [w, h], [w, h]];
  for (let f = 0; f < 6; f++) {
    for (let k = 0; k < 4; k++) {
      const i = f * 4 + k;
      uv.setXY(i, (uv.getX(i) * dims[f][0]) / tile, (uv.getY(i) * dims[f][1]) / tile);
    }
  }
  uv.needsUpdate = true;
}

/** `boxes` is map_boxes(id): 7 a box. `bounds` is min x, min z, max x,
 * max z. */
/** Takes a level built by buildWorld out of the scene. */
export function dropWorld(level: THREE.Group) {
  level.removeFromParent();
  level.traverse((o) => {
    if (o instanceof THREE.Mesh) o.geometry.dispose();
  });
}

/** The ground mesh from map_terrain: the sim's vertices and its split of
 * each cell (low corner to high corner), coloured by height and slope. */
function terrainMesh(t: Float32Array, mat: THREE.Material): THREE.Mesh {
  const [cell, x0, z0, nx, nz] = [t[0], t[1], t[2], t[3], t[4]];
  const h = (i: number, j: number) => t[5 + j * nx + i];
  const pos: number[] = [], col: number[] = [], idx: number[] = [];
  const sand = new THREE.Color(0xc49a6c), grass = new THREE.Color(0x6e7b3a), rock = new THREE.Color(0x9c5536);
  for (let j = 0; j < nz; j++) {
    for (let i = 0; i < nx; i++) {
      pos.push(x0 + i * cell, h(i, j), z0 + j * cell);
      // Slope from the neighbours: steep is rock, flat and high is grass.
      const dx = (h(Math.min(i + 1, nx - 1), j) - h(Math.max(i - 1, 0), j)) / (2 * cell);
      const dz = (h(i, Math.min(j + 1, nz - 1)) - h(i, Math.max(j - 1, 0))) / (2 * cell);
      const steep = Math.min(1, Math.hypot(dx, dz) / 0.8);
      const c = sand.clone().lerp(grass, Math.min(1, Math.max(0, h(i, j) * 0.6))).lerp(rock, steep);
      col.push(c.r, c.g, c.b);
    }
  }
  for (let j = 0; j + 1 < nz; j++) {
    for (let i = 0; i + 1 < nx; i++) {
      const a = j * nx + i, b = a + 1, c = a + nx, d = c + 1;
      // Upper (x offset ≥ z offset): a, b, d. Lower: a, d, c. Wound to face up.
      idx.push(a, d, b, a, c, d);
    }
  }
  const g = new THREE.BufferGeometry();
  g.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
  g.setAttribute('color', new THREE.Float32BufferAttribute(col, 3));
  g.setIndex(idx);
  g.computeVertexNormals();
  const m = new THREE.Mesh(g, mat);
  m.receiveShadow = true;
  return m;
}

export function buildWorld(scene: THREE.Scene, boxes: Float32Array, pads: Float32Array, mats: Materials,
  bounds: ArrayLike<number> = [-24, -24, 24, 24], terrain: Float32Array = new Float32Array(0),
  launchers: Float32Array = new Float32Array(0)) {
  // Everything the level adds goes in one group, so a new map (a cup's next
  // heat) replaces it whole. The sky and the fog are the scene's own.
  const root = scene;
  const level = new THREE.Group();
  scene.add(level);
  scene = level as unknown as THREE.Scene;
  const [bx0, bz0, bx1, bz1] = Array.from(bounds).slice(0, 4);
  const cx = (bx0 + bx1) / 2, cz = (bz0 + bz1) / 2;
  const half = Math.max(bx1 - bx0, bz1 - bz0) / 2 + 8;
  root.background = new THREE.Color(0x9fc4e8);
  root.fog = new THREE.Fog(0x9fc4e8, half * 1.5, half * 4);

  const hemi = new THREE.HemisphereLight(0xdfefff, 0x4a4036, 1.1);
  scene.add(hemi);
  const sun = new THREE.DirectionalLight(0xfff2dd, 2.2);
  sun.position.set(-30, 50, 20);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  const sc = sun.shadow.camera as THREE.OrthographicCamera;
  sc.left = sc.bottom = -half;
  sc.right = sc.top = half;
  sun.position.set(cx - 30, 50, cz + 20);
  sun.target.position.set(cx, 0, cz);
  scene.add(sun.target);
  sc.near = 1;
  sc.far = Math.max(120, half * 3);
  sun.shadow.bias = -0.0005;
  scene.add(sun);

  if (terrain.length > 5) {
    scene.add(terrainMesh(terrain, mats.ground));
  } else {
    const size = 2 * half + 40;
    const fg = new THREE.PlaneGeometry(size, size);
    const fuv = fg.attributes.uv as THREE.BufferAttribute;
    for (let i = 0; i < fuv.count; i++) fuv.setXY(i, fuv.getX(i) * size / 4, fuv.getY(i) * size / 4);
    const floor = new THREE.Mesh(fg, mats.floor);
    floor.rotation.x = -Math.PI / 2;
    floor.position.set(cx, 0, cz);
    floor.receiveShadow = true;
    scene.add(floor);
  }
  // Launchers: a glowing plate where the throw starts.
  const glow = new THREE.MeshStandardMaterial({ color: 0x46d0ff, emissive: 0x1a8fc0, roughness: 0.4 });
  for (let i = 0; i + 9 <= launchers.length; i += 9) {
    const [x0, y0, z0, x1, , z1] = launchers.subarray(i, i + 6);
    const plate = new THREE.Mesh(new THREE.BoxGeometry(x1 - x0, 0.08, z1 - z0), glow);
    plate.position.set((x0 + x1) / 2, y0 + 0.04, (z0 + z1) / 2);
    plate.receiveShadow = true;
    scene.add(plate);
  }

  for (let i = 0; i + 7 <= boxes.length; i += 7) {
    const [x0, y0, z0, x1, y1, z1, look] = boxes.subarray(i, i + 7);
    const w = x1 - x0, h = y1 - y0, d = z1 - z0;
    const kind = LOOKS[look] ?? 'block';
    const tall = kind === 'wall';
    const g = new THREE.BoxGeometry(w, h, d);
    tileUVs(g, w, h, d, tall ? 4 : 2);
    const m = new THREE.Mesh(g, mats[kind]);
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
  return { level, sun, padMeshes };
}
