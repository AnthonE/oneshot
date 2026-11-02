// Models, viewmodel and sound: loaded after the level so the game is
// playable while they come. Filled in with Triple T and Gates' guns.

import type * as THREE from 'three';
import type { Players } from './players';
import type { Frame } from './input';

export interface Art {
  frame(own: Float32Array, f: Frame, dt: number, now: number): void;
  shot(mine: boolean, arm: number, at: THREE.Vector3): void;
  hit(mine: boolean, part: number): void;
  blast(at: THREE.Vector3): void;
}

export async function loadArt(
  _scene: THREE.Scene,
  _camera: THREE.Camera,
  _renderer: THREE.WebGLRenderer,
  _players: Players,
): Promise<Art | null> {
  return null;
}
