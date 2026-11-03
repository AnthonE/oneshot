/* tslint:disable */
/* eslint-disable */

export class Client {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    close(): void;
    /**
     * Connects to `addr` (host:port), pinning the certificate whose
     * SHA-256 is `hash` (hex; empty for a certificate from a CA, which the
     * browser checks against its own roots), and asks to play `mode` (0 casual against
     * a bot, 1 ranked Duel, 2 ranked Doubles, 3 free for all, 4 Gun Game,
     * 5 a cup)
     * as `name` wearing `skin` (worn only if the pass's wallet owns it),
     * signed in with an
     * Elo Pros `pass` (empty for a guest). Returns once the hello is sent:
     * the welcome comes when a room is ready (at once for casual, when the
     * queue forms a match for ranked), and `ready` turns true. A refusal
     * comes through `messages()`.
     */
    static connect(addr: string, hash: string, name: string, pass: string, mode: number, skin: number): Promise<Client>;
    /**
     * What happened since the last call, 11 each: kind, a, b, part, c,
     * pos x y z, end x y z (sim::events::Kind numbers the kinds).
     */
    events(): Float32Array;
    /**
     * While we are dead and saw who killed us: the kill cam, 7 numbers
     * then the bodies as `others()` gives them (ourselves among them, the
     * killer not): eye x y z, yaw, pitch (radians), the killer's seat, how
     * far through the run-up (0..1). Empty otherwise. Made only of
     * snapshots we were already sent.
     */
    killcam(now: number): Float32Array;
    /**
     * The match: phase (0 countdown, 1 playing, 2 over), seconds left,
     * winner, the score limit, the number of scores `n`, then the `n`
     * scores (each side's with teams, else each seat's), then each pad's
     * seconds until its gun is back.
     */
    match_state(): Float32Array;
    /**
     * Stream messages since the last call, as JS objects.
     */
    messages(): Array<any>;
    /**
     * Everyone else, 14 numbers each: slot, team, alive, crouched,
     * grounded, reloading, x, y, z, yaw, pitch, arm, shield (0..1), speed.
     */
    others(now: number): Float32Array;
    /**
     * Our own fighter, drawn: x y z, eye height, yaw, pitch (radians),
     * alive, crouched, shield, health (points), arm, mag, reserve, other
     * arm, grenades, reloading (0..1), team, kills, deaths, respawn (s),
     * grounded, speed, then the seat it rides in (−1 on foot, 0 driver, 1
     * passenger) and that vehicle's slot, then the gun's heat in % (−1 for
     * a gun that doesn't heat), the left hand's arm (0: none), mag and
     * reserve, stickies, and the grenade a throw takes (0 frag, 1 sticky).
     */
    own(): Float32Array;
    /**
     * Rockets and frags, 7 each: kind (0 rocket, 1 frag), owner, x y z,
     * velocity x z.
     */
    projectiles(): Float32Array;
    /**
     * The gun on Gun Game's rung `score`.
     */
    rung(score: number): number;
    /**
     * Netcode health: snapshots, confirmations, corrections, resyncs,
     * depth, clock rate, interpolation delay (ticks), datagrams dropped,
     * and whether the match's revealed salt matched its commitment (1 yes,
     * 0 no, −1 not revealed yet).
     */
    stats(): Float64Array;
    /**
     * Once a frame: takes in what arrived, advances the clock with the
     * controls as they are, sends what is due.
     */
    update(now: number, buttons: number, yaw: number, pitch: number, mx: number, mz: number): void;
    /**
     * The vehicles to draw, 8 numbers each: slot, x y z (the hull's middle
     * at the ground), yaw (radians), health (0..1), and each seat's rider
     * (a slot, or −1). The one we ride in is our prediction of it.
     */
    vehicles(now: number): Float32Array;
    readonly closed: boolean;
    /**
     * Whether this match is Gun Game: the score is the rung.
     */
    readonly gun_game: boolean;
    /**
     * The map this match is on (`map_boxes(map)` and friends draw it).
     */
    readonly map: number;
    /**
     * Whether the welcome has come and play has begun.
     */
    readonly ready: boolean;
    readonly slot: number;
    readonly team: number;
    /**
     * Whether this match is red against blue.
     */
    readonly teams: boolean;
    /**
     * How many welcomes have come: a cup's entrant is welcomed into each
     * heat on the same connection, and the page redraws the level then.
     */
    readonly welcomes: number;
}

/**
 * Arm names by id, for the HUD.
 */
export function arm_name(id: number): string;

/**
 * Whether arm `id`'s rounds are rays (drawn as tracers), not projectiles.
 */
export function arm_ray(id: number): boolean;

/**
 * Map `id`'s boxes, 7 numbers each (min x y z, max x y z, look): the one
 * source of the level's geometry for the renderer.
 */
export function map_boxes(id: number): Float32Array;

/**
 * Map `id`'s launchers, 9 each: the pad's min x y z, max x y z, then the
 * velocity it throws at.
 */
export function map_launchers(id: number): Float32Array;

/**
 * Map `id`'s bounds (min x, min z, max x, max z), then its roam points
 * (x, z each).
 */
export function map_layout(id: number): Float32Array;

/**
 * Map `id`'s name.
 */
export function map_name(id: number): string;

/**
 * Map `id`'s pads, 5 each: x y z, arm, respawn seconds.
 */
export function map_pads(id: number): Float32Array;

/**
 * Map `id`'s ground, if it isn't flat: cell size, corner x, corner z,
 * vertices along x, along z, then the heights in metres, row by row (z
 * outer). Empty for a flat floor. Each cell is two triangles split from
 * its low corner to its high one, as the sim's are (`sim::terrain`): the
 * one where the x offset is at least the z offset, then the other.
 */
export function map_terrain(id: number): Float32Array;

/**
 * Skin names by id (`proto::SKINS`), for the menu.
 */
export function skin_names(): string[];

export function start(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_client_free: (a: number, b: number) => void;
    readonly arm_name: (a: number) => [number, number];
    readonly arm_ray: (a: number) => number;
    readonly client_close: (a: number) => void;
    readonly client_closed: (a: number) => number;
    readonly client_connect: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => any;
    readonly client_events: (a: number) => [number, number];
    readonly client_gun_game: (a: number) => number;
    readonly client_killcam: (a: number, b: number) => [number, number];
    readonly client_map: (a: number) => number;
    readonly client_match_state: (a: number) => [number, number];
    readonly client_messages: (a: number) => any;
    readonly client_others: (a: number, b: number) => [number, number];
    readonly client_own: (a: number) => [number, number];
    readonly client_projectiles: (a: number) => [number, number];
    readonly client_ready: (a: number) => number;
    readonly client_rung: (a: number, b: number) => number;
    readonly client_slot: (a: number) => number;
    readonly client_stats: (a: number) => [number, number];
    readonly client_team: (a: number) => number;
    readonly client_teams: (a: number) => number;
    readonly client_update: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly client_vehicles: (a: number, b: number) => [number, number];
    readonly client_welcomes: (a: number) => number;
    readonly map_boxes: (a: number) => [number, number];
    readonly map_launchers: (a: number) => [number, number];
    readonly map_layout: (a: number) => [number, number];
    readonly map_name: (a: number) => [number, number];
    readonly map_pads: (a: number) => [number, number];
    readonly map_terrain: (a: number) => [number, number];
    readonly skin_names: () => [number, number];
    readonly start: () => void;
    readonly wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___js_sys_156111757ae93529___Function_fn_wasm_bindgen_d2d7274a6b243f3a___JsValue_____wasm_bindgen_d2d7274a6b243f3a___sys__Undefined___js_sys_156111757ae93529___Function_fn_wasm_bindgen_d2d7274a6b243f3a___JsValue_____wasm_bindgen_d2d7274a6b243f3a___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___JsValue__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___sys__Undefined__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___sys__Undefined__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true__34: (a: number, b: number, c: any) => [number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
