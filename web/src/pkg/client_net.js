/* @ts-self-types="./client_net.d.ts" */

export class Client {
    static __wrap(ptr) {
        const obj = Object.create(Client.prototype);
        obj.__wbg_ptr = ptr;
        ClientFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ClientFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_client_free(ptr, 0);
    }
    close() {
        wasm.client_close(this.__wbg_ptr);
    }
    /**
     * @returns {boolean}
     */
    get closed() {
        const ret = wasm.client_closed(this.__wbg_ptr);
        return ret !== 0;
    }
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
     * @param {string} addr
     * @param {string} hash
     * @param {string} name
     * @param {string} pass
     * @param {number} mode
     * @param {number} skin
     * @returns {Promise<Client>}
     */
    static connect(addr, hash, name, pass, mode, skin) {
        const val5 = +mode;
        const val6 = +skin;
        let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3;
        try {
            ptr0 = passStringToWasm0(addr, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(hash, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            len1 = WASM_VECTOR_LEN;
            ptr2 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            len2 = WASM_VECTOR_LEN;
            ptr3 = passStringToWasm0(pass, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            len3 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr3) { wasm.__wbindgen_free(ptr3, len3 * 1, 1); }
            if (ptr2) { wasm.__wbindgen_free(ptr2, len2 * 1, 1); }
            if (ptr1) { wasm.__wbindgen_free(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_free(ptr0, len0 * 1, 1); }
            throw e;
        }
        const ret = wasm.client_connect(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, val5, val6);
        return ret;
    }
    /**
     * What happened since the last call, 11 each: kind, a, b, part, c,
     * pos x y z, end x y z (sim::events::Kind numbers the kinds).
     * @returns {Float32Array}
     */
    events() {
        const ret = wasm.client_events(this.__wbg_ptr);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Whether this match is Gun Game: the score is the rung.
     * @returns {boolean}
     */
    get gun_game() {
        const ret = wasm.client_gun_game(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * While we are dead and saw who killed us: the kill cam, 7 numbers
     * then the bodies as `others()` gives them (ourselves among them, the
     * killer not): eye x y z, yaw, pitch (radians), the killer's seat, how
     * far through the run-up (0..1). Empty otherwise. Made only of
     * snapshots we were already sent.
     * @param {number} now
     * @returns {Float32Array}
     */
    killcam(now) {
        const ret = wasm.client_killcam(this.__wbg_ptr, now);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * The map this match is on (`map_boxes(map)` and friends draw it).
     * @returns {number}
     */
    get map() {
        const ret = wasm.client_map(this.__wbg_ptr);
        return ret;
    }
    /**
     * The match: phase (0 countdown, 1 playing, 2 over), seconds left,
     * winner, the score limit, the number of scores `n`, then the `n`
     * scores (each side's with teams, else each seat's), then each pad's
     * seconds until its gun is back.
     * @returns {Float32Array}
     */
    match_state() {
        const ret = wasm.client_match_state(this.__wbg_ptr);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Stream messages since the last call, as JS objects.
     * @returns {Array<any>}
     */
    messages() {
        const ret = wasm.client_messages(this.__wbg_ptr);
        return ret;
    }
    /**
     * Everyone else, 14 numbers each: slot, team, alive, crouched,
     * grounded, reloading, x, y, z, yaw, pitch, arm, shield (0..1), speed.
     * @param {number} now
     * @returns {Float32Array}
     */
    others(now) {
        const ret = wasm.client_others(this.__wbg_ptr, now);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Our own fighter, drawn: x y z, eye height, yaw, pitch (radians),
     * alive, crouched, shield, health (points), arm, mag, reserve, other
     * arm, grenades, reloading (0..1), team, kills, deaths, respawn (s),
     * grounded, speed, then the seat it rides in (−1 on foot, 0 driver, 1
     * passenger) and that vehicle's slot, then the gun's heat in % (−1 for
     * a gun that doesn't heat), the left hand's arm (0: none), mag and
     * reserve, stickies, and the grenade a throw takes (0 frag, 1 sticky).
     * @returns {Float32Array}
     */
    own() {
        const ret = wasm.client_own(this.__wbg_ptr);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Rockets and frags, 7 each: kind (0 rocket, 1 frag), owner, x y z,
     * velocity x z.
     * @returns {Float32Array}
     */
    projectiles() {
        const ret = wasm.client_projectiles(this.__wbg_ptr);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Whether the welcome has come and play has begun.
     * @returns {boolean}
     */
    get ready() {
        const ret = wasm.client_ready(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * The gun on Gun Game's rung `score`.
     * @param {number} score
     * @returns {number}
     */
    rung(score) {
        const ret = wasm.client_rung(this.__wbg_ptr, score);
        return ret;
    }
    /**
     * @returns {number}
     */
    get slot() {
        const ret = wasm.client_slot(this.__wbg_ptr);
        return ret;
    }
    /**
     * Netcode health: snapshots, confirmations, corrections, resyncs,
     * depth, clock rate, interpolation delay (ticks), datagrams dropped,
     * and whether the match's revealed salt matched its commitment (1 yes,
     * 0 no, −1 not revealed yet).
     * @returns {Float64Array}
     */
    stats() {
        const ret = wasm.client_stats(this.__wbg_ptr);
        var v1 = getArrayF64FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 8, 8);
        return v1;
    }
    /**
     * @returns {number}
     */
    get team() {
        const ret = wasm.client_team(this.__wbg_ptr);
        return ret;
    }
    /**
     * Whether this match is red against blue.
     * @returns {boolean}
     */
    get teams() {
        const ret = wasm.client_teams(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Once a frame: takes in what arrived, advances the clock with the
     * controls as they are, sends what is due.
     * @param {number} now
     * @param {number} buttons
     * @param {number} yaw
     * @param {number} pitch
     * @param {number} mx
     * @param {number} mz
     */
    update(now, buttons, yaw, pitch, mx, mz) {
        wasm.client_update(this.__wbg_ptr, now, buttons, yaw, pitch, mx, mz);
    }
    /**
     * The vehicles to draw, 8 numbers each: slot, x y z (the hull's middle
     * at the ground), yaw (radians), health (0..1), and each seat's rider
     * (a slot, or −1). The one we ride in is our prediction of it.
     * @param {number} now
     * @returns {Float32Array}
     */
    vehicles(now) {
        const ret = wasm.client_vehicles(this.__wbg_ptr, now);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * How many welcomes have come: a cup's entrant is welcomed into each
     * heat on the same connection, and the page redraws the level then.
     * @returns {number}
     */
    get welcomes() {
        const ret = wasm.client_welcomes(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) Client.prototype[Symbol.dispose] = Client.prototype.free;

/**
 * Arm names by id, for the HUD.
 * @param {number} id
 * @returns {string}
 */
export function arm_name(id) {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.arm_name(id);
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Whether arm `id`'s rounds are rays (drawn as tracers), not projectiles.
 * @param {number} id
 * @returns {boolean}
 */
export function arm_ray(id) {
    const ret = wasm.arm_ray(id);
    return ret !== 0;
}

/**
 * Map `id`'s boxes, 7 numbers each (min x y z, max x y z, look): the one
 * source of the level's geometry for the renderer.
 * @param {number} id
 * @returns {Float32Array}
 */
export function map_boxes(id) {
    const ret = wasm.map_boxes(id);
    var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    return v1;
}

/**
 * Map `id`'s launchers, 9 each: the pad's min x y z, max x y z, then the
 * velocity it throws at.
 * @param {number} id
 * @returns {Float32Array}
 */
export function map_launchers(id) {
    const ret = wasm.map_launchers(id);
    var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    return v1;
}

/**
 * Map `id`'s bounds (min x, min z, max x, max z), then its roam points
 * (x, z each).
 * @param {number} id
 * @returns {Float32Array}
 */
export function map_layout(id) {
    const ret = wasm.map_layout(id);
    var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    return v1;
}

/**
 * Map `id`'s name.
 * @param {number} id
 * @returns {string}
 */
export function map_name(id) {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.map_name(id);
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Map `id`'s pads, 5 each: x y z, arm, respawn seconds.
 * @param {number} id
 * @returns {Float32Array}
 */
export function map_pads(id) {
    const ret = wasm.map_pads(id);
    var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    return v1;
}

/**
 * Map `id`'s ground, if it isn't flat: cell size, corner x, corner z,
 * vertices along x, along z, then the heights in metres, row by row (z
 * outer). Empty for a flat floor. Each cell is two triangles split from
 * its low corner to its high one, as the sim's are (`sim::terrain`): the
 * one where the x offset is at least the z offset, then the other.
 * @param {number} id
 * @returns {Float32Array}
 */
export function map_terrain(id) {
    const ret = wasm.map_terrain(id);
    var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    return v1;
}

/**
 * Skin names by id (`proto::SKINS`), for the menu.
 * @returns {string[]}
 */
export function skin_names() {
    const ret = wasm.skin_names();
    var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]);
    wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
    return v1;
}

export function start() {
    wasm.start();
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_208446807d0d1e30: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_boolean_get_4f1baf9a00be3c74: function(arg0) {
            const v = arg0;
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_is_function_741ff2bb612836f5: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_undefined_be5b34249ff1fc15: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_string_get_7ce8eee7fd327545: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_4c88d8bf264fd984: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg__wbg_cb_unref_09736879a3f67559: function(arg0) {
            arg0._wbg_cb_unref();
        },
        __wbg_call_40b008ba32f73991: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.call(arg1);
            return ret;
        }, arguments); },
        __wbg_call_a410eb4b59ed5965: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_client_new: function(arg0) {
            const ret = Client.__wrap(arg0);
            return ret;
        },
        __wbg_close_cee6fa489413fa4f: function(arg0) {
            arg0.close();
        },
        __wbg_createBidirectionalStream_5e43809fe9256e57: function(arg0) {
            const ret = arg0.createBidirectionalStream();
            return ret;
        },
        __wbg_datagrams_c31e7bb4b7300ae1: function(arg0) {
            const ret = arg0.datagrams;
            return ret;
        },
        __wbg_desiredSize_322d65cdd5dbaec8: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.desiredSize;
            getDataViewMemory0().setFloat64((arg0 >>> 0) + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, !isLikeNone(ret), true);
        }, arguments); },
        __wbg_error_d1fc0193610e0e39: function(arg0) {
            console.error(arg0);
        },
        __wbg_getWriter_174052e9b400c61b: function() { return handleError(function (arg0) {
            const ret = arg0.getWriter();
            return ret;
        }, arguments); },
        __wbg_get_2080c49015e35254: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_instanceof_Uint8Array_d7aeb0dcb3108fea: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Window_5cf453f5639aa1a2: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Window;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_length_186163212ffe79c3: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_maxDatagramSize_f31d931ff88c7b74: function(arg0) {
            const ret = arg0.maxDatagramSize;
            return ret;
        },
        __wbg_new_37eb45aaf5eeb820: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_new_418047748c820cbb: function() { return handleError(function (arg0) {
            const ret = new ReadableStreamDefaultReader(arg0);
            return ret;
        }, arguments); },
        __wbg_new_cd4348290afae586: function() {
            const ret = new Object();
            return ret;
        },
        __wbg_new_typed_e09da9dae6ee5935: function(arg0, arg1) {
            try {
                var state0 = {a: arg0, b: arg1};
                var cb0 = (arg0, arg1) => {
                    const a = state0.a;
                    state0.a = 0;
                    try {
                        return wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___js_sys_156111757ae93529___Function_fn_wasm_bindgen_d2d7274a6b243f3a___JsValue_____wasm_bindgen_d2d7274a6b243f3a___sys__Undefined___js_sys_156111757ae93529___Function_fn_wasm_bindgen_d2d7274a6b243f3a___JsValue_____wasm_bindgen_d2d7274a6b243f3a___sys__Undefined_______true_(a, state0.b, arg0, arg1);
                    } finally {
                        state0.a = a;
                    }
                };
                const ret = new Promise(cb0);
                return ret;
            } finally {
                state0.a = 0;
            }
        },
        __wbg_new_with_length_01b143a7e5dd5a3e: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return ret;
        },
        __wbg_new_with_options_2b7d4e52be27db43: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = new WebTransport(getStringFromWasm0(arg0, arg1), arg2);
            return ret;
        }, arguments); },
        __wbg_now_043d245551b22f9c: function(arg0) {
            const ret = arg0.now();
            return ret;
        },
        __wbg_performance_50e6a3f8dc80bbca: function(arg0) {
            const ret = arg0.performance;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_prototypesetcall_13162965d1fe7aa3: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_push_c9f4893fc209f768: function(arg0, arg1) {
            const ret = arg0.push(arg1);
            return ret;
        },
        __wbg_queueMicrotask_1e8503a8a690f0a3: function(arg0) {
            const ret = arg0.queueMicrotask;
            return ret;
        },
        __wbg_queueMicrotask_4a1a4d8dd332373b: function(arg0) {
            queueMicrotask(arg0);
        },
        __wbg_read_e3bea75dace70752: function(arg0) {
            const ret = arg0.read();
            return ret;
        },
        __wbg_readable_31a21a592117aaea: function(arg0) {
            const ret = arg0.readable;
            return ret;
        },
        __wbg_readable_9d837f70281b731e: function(arg0) {
            const ret = arg0.readable;
            return ret;
        },
        __wbg_ready_82152a859e59ead5: function(arg0) {
            const ret = arg0.ready;
            return ret;
        },
        __wbg_resolve_dbf78ab4942478b0: function(arg0) {
            const ret = Promise.resolve(arg0);
            return ret;
        },
        __wbg_set_2793479888329cb6: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = Reflect.set(arg0, arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_set_algorithm_b96693873908739b: function(arg0, arg1, arg2) {
            arg0.algorithm = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_feb3714f475485ba: function(arg0, arg1, arg2) {
            arg0.set(getArrayU8FromWasm0(arg1, arg2));
        },
        __wbg_set_require_unreliable_04052127906c4e75: function(arg0, arg1) {
            arg0.requireUnreliable = arg1 !== 0;
        },
        __wbg_set_server_certificate_hashes_5c2e061cc49c13ee: function(arg0, arg1, arg2) {
            arg0.serverCertificateHashes = getArrayJsValueViewFromWasm0(arg1, arg2);
        },
        __wbg_set_value_u8_array_c85e8426d0802519: function(arg0, arg1) {
            arg0.value = arg1;
        },
        __wbg_static_accessor_GLOBAL_THIS_003ee833af9699fc: function() {
            const ret = typeof globalThis === 'undefined' ? undefined : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_e4293fb22cbe77da: function() {
            const ret = typeof global === 'undefined' ? undefined : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_53bb20457401453d: function() {
            const ret = typeof self === 'undefined' ? undefined : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_b5e0810bfecb7feb: function() {
            const ret = typeof window === 'undefined' ? undefined : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_then_656f253a5dd7d8ea: function(arg0, arg1, arg2) {
            const ret = arg0.then(arg1, arg2);
            return ret;
        },
        __wbg_then_e3f7f2d365708be8: function(arg0, arg1) {
            const ret = arg0.then(arg1);
            return ret;
        },
        __wbg_writable_0b74e9fe28f09968: function(arg0) {
            const ret = arg0.writable;
            return ret;
        },
        __wbg_write_eb694c46174356a6: function(arg0, arg1) {
            const ret = arg0.write(arg1);
            return ret;
        },
        __wbindgen_generic_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 25, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___JsValue__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true_);
            return ret;
        },
        __wbindgen_generic_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("WebTransportBidirectionalStream")], shim_idx: 1, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___sys__Undefined__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true_);
            return ret;
        },
        __wbindgen_generic_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("undefined")], shim_idx: 1, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___sys__Undefined__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true__34);
            return ret;
        },
        __wbindgen_generic_0000000000000004: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_generic_0000000000000005: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            table.set(0, undefined);
            table.set(1024, undefined);
            table.set(1025, null);
            table.set(1026, true);
            table.set(1027, false);
        },
    };
    return {
        __proto__: null,
        "./client_net_bg.js": import0,
    };
}

function wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___JsValue__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true_(arg0, arg1, arg2) {
    const ret = wasm.wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___JsValue__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true_(arg0, arg1, arg2);
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

function wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___sys__Undefined__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true_(arg0, arg1, arg2) {
    const ret = wasm.wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___sys__Undefined__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true_(arg0, arg1, arg2);
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

function wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___sys__Undefined__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true__34(arg0, arg1, arg2) {
    const ret = wasm.wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___wasm_bindgen_d2d7274a6b243f3a___sys__Undefined__core_7d5f0a2ba6a62c33___result__Result_____wasm_bindgen_d2d7274a6b243f3a___JsError___true__34(arg0, arg1, arg2);
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

function wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___js_sys_156111757ae93529___Function_fn_wasm_bindgen_d2d7274a6b243f3a___JsValue_____wasm_bindgen_d2d7274a6b243f3a___sys__Undefined___js_sys_156111757ae93529___Function_fn_wasm_bindgen_d2d7274a6b243f3a___JsValue_____wasm_bindgen_d2d7274a6b243f3a___sys__Undefined_______true_(arg0, arg1, arg2, arg3) {
    wasm.wasm_bindgen_d2d7274a6b243f3a___convert__closures_____invoke___js_sys_156111757ae93529___Function_fn_wasm_bindgen_d2d7274a6b243f3a___JsValue_____wasm_bindgen_d2d7274a6b243f3a___sys__Undefined___js_sys_156111757ae93529___Function_fn_wasm_bindgen_d2d7274a6b243f3a___JsValue_____wasm_bindgen_d2d7274a6b243f3a___sys__Undefined_______true_(arg0, arg1, arg2, arg3);
}

const ClientFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_client_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(state => wasm.__wbindgen_destroy_closure(state.a, state.b));

function getArrayF32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayF64FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat64ArrayMemory0().subarray(ptr / 8, ptr / 8 + len);
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

function getArrayJsValueViewFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
    if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
        cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
    }
    return cachedFloat32ArrayMemory0;
}

let cachedFloat64ArrayMemory0 = null;
function getFloat64ArrayMemory0() {
    if (cachedFloat64ArrayMemory0 === null || cachedFloat64ArrayMemory0.byteLength === 0) {
        cachedFloat64ArrayMemory0 = new Float64Array(wasm.memory.buffer);
    }
    return cachedFloat64ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function makeMutClosure(arg0, arg1, f) {
    const state = { a: arg0, b: arg1, cnt: 1 };
    const real = (...args) => {

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            state.a = a;
            real._wbg_cb_unref();
        }
    };
    real._wbg_cb_unref = () => {
        if (--state.cnt === 0) {
            wasm.__wbindgen_destroy_closure(state.a, state.b);
            state.a = 0;
            CLOSURE_DTORS.unregister(state);
        }
    };
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(bytes) {
    numBytesDecoded += bytes.length;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
        cachedTextDecoder.decode();
        numBytesDecoded = bytes.length;
    }
    return cachedTextDecoder.decode(bytes);
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat32ArrayMemory0 = null;
    cachedFloat64ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

let __wbg_init_promise;
async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;
    if (__wbg_init_promise === undefined) {
        __wbg_init_promise = __wbg_init_once(module_or_path);
        __wbg_init_promise.catch(() => { __wbg_init_promise = undefined; });
    }
    return __wbg_init_promise;
}

async function __wbg_init_once(module_or_path) {

    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('client_net_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
