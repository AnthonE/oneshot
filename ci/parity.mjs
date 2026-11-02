// node ci/parity.mjs <sim_wasm.wasm> <seed> <ticks>: the folded hash of a bot
// match run by the sim compiled to wasm, printed as 16 hex digits.
import { readFileSync } from 'node:fs';

const [file, seed = '1', ticks = '20000'] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(file), {});
const h = instance.exports.bot_match(BigInt(seed), Number(ticks));
console.log(BigInt.asUintN(64, h).toString(16).padStart(16, '0'));
