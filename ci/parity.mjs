// node ci/parity.mjs <sim_wasm.wasm> <seed> <ticks> [map mode]: the
// folded hash of a bot match run by the sim compiled to wasm, printed as 16
// hex digits. With a map: on that map in `mode` (0 Doubles, 1 an
// eight-seat Gun Game, 2 a sixteen-seat Big Team).
import { readFileSync } from 'node:fs';

const [file, seed = '1', ticks = '20000', map, mode = '0'] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(file), {});
const h = map === undefined
  ? instance.exports.bot_match(BigInt(seed), Number(ticks))
  : instance.exports.bot_match_on(Number(map), Number(mode), BigInt(seed), Number(ticks));
console.log(BigInt.asUintN(64, h).toString(16).padStart(16, '0'));
