//! Every number the rules run on. Durations are milliseconds and speeds are
//! per second; [`ticks`] turns a duration into ticks from [`TICK_HZ`], so
//! the tick rate is one constant (docs/DESIGN.md §3).

/// Sim ticks a second, on the server and in the browser's prediction.
pub const TICK_HZ: u32 = 128;
/// One tick in seconds. 1/128 is a power of two, so it is exact.
pub const DT: f32 = 1.0 / TICK_HZ as f32;

/// A duration in ms as whole ticks, rounded to nearest.
pub const fn ticks(ms: u32) -> u32 {
    (ms * TICK_HZ + 500) / 1000
}

/// Seats in a room: Duel 2, Doubles 4, a free for all or a cup heat 8,
/// Big Team 16.
pub const MAX_FIGHTERS: usize = 16;
/// Rockets and frags in flight at once.
pub const MAX_PROJ: usize = 32;
/// Weapon pads on a map.
pub const MAX_PADS: usize = 8;
/// Events one tick may raise; past it they are dropped and counted.
pub const MAX_EVENTS: usize = 128;
/// Boxes in a map (`tools/maps/compile.mjs` holds maps to it).
pub const MAX_MAP_BOXES: usize = 256;
/// Ticks of pose history for lag compensation (250 ms at 128 Hz).
pub const REWIND_TICKS: usize = 32;

// Movement: Quake's model (Fireplace's `PHYS`), restated per second.
pub const RUN: f32 = 7.0;
pub const CROUCH_RUN: f32 = 3.5;
/// Below this speed friction acts as if moving at it, so a body stops.
pub const STOP: f32 = 2.5;
pub const FRICTION: f32 = 6.0;
pub const ACCEL: f32 = 10.0;
pub const AIR_ACCEL: f32 = 10.0;
/// Quake's air wish-speed cap: air strafing works, air running doesn't.
pub const AIR_CAP: f32 = 1.0;
pub const GRAVITY: f32 = 20.0;
/// Take-off speed: an apex of v²/2g = 1.225 m.
pub const JUMP_SPEED: f32 = 7.0;
pub const VMAX: f32 = 60.0;

// The body: a box for walking, a cylinder and a sphere for being shot.
pub const BODY_R: f32 = 0.35;
pub const BODY_H: f32 = 1.8;
pub const CROUCH_H: f32 = 1.3;
pub const EYE: f32 = 1.6;
pub const CROUCH_EYE: f32 = 1.1;
/// A grounded body walks up onto anything this much higher than its feet.
pub const STEP: f32 = 0.5;
/// A grounded body walking off a ledge this low stays on the ground.
pub const SNAP: f32 = 0.25;
/// Ground steeper than 46° can't be stood on or walked up: the normal's
/// height below this (cos 46°) slides the body, and a rise steeper than
/// this per metre (tan 46°) stops a step up it.
pub const WALK_NY: f32 = 0.694_658_4;
pub const MAX_CLIMB: f32 = 1.035_530_4;
/// How hard a too-steep slope pushes a body down it, in m/s².
pub const SLIDE_ACCEL: f32 = 14.0;
pub const HIT_R: f32 = 0.35;
pub const HIT_TOP: f32 = 1.45;
pub const HIT_TOP_CROUCH: f32 = 0.95;
pub const HEAD_R: f32 = 0.22;
pub const HEAD_Y: f32 = 1.62;
pub const HEAD_Y_CROUCH: f32 = 1.12;
/// Pitch limit in 1/65536 turn: 89°.
pub const PITCH_MAX: i16 = 16_202;

/// Positions and velocities are kept on a grid of 1/1024 m (m/s), every
/// tick, on both sides, so a prediction and the server agree to the bit
/// and the wire can carry the client's own body exactly.
pub const QUANT: f32 = 1024.0;

// Shields and health in 1/256 point, so regen per tick is whole.
pub const PT: i32 = 256;
pub const SHIELD_MAX: i32 = 100 * PT;
pub const HEALTH_MAX: i32 = 100 * PT;
pub const SHIELD_DELAY_MS: u32 = 5000;
/// Points a second once recharging (full in 1.67 s).
pub const SHIELD_RATE: i32 = 60;
/// Health comes back once shields are full.
pub const HEALTH_RATE: i32 = 30;
pub const RESPAWN_MS: u32 = 3000;
/// Spawn points drawn when picking where someone comes back.
pub const SPAWN_DRAWS: usize = 4;

pub const MELEE_DMG: i32 = 70;
pub const MELEE_RANGE: f32 = 2.0;
pub const MELEE_COOL_MS: u32 = 800;
/// A hit within this cosine of straight behind the victim kills.
pub const BEHIND_COS: f32 = 0.5;

// The sword (arms::SWORD). A pull with the aim on someone (its body grown
// by `LUNGE_PAD`: the red reticle) kills them in `SWORD_REACH`, or past it
// to `LUNGE_RANGE` lunges: the world drives the body at them, steering, for
// up to `LUNGE_MS`, and kills on reaching `STRIKE_R` of them. Two lunging
// at each other clash: both thrown back, nobody hurt.
pub const SWORD_REACH: f32 = 2.0;
pub const LUNGE_RANGE: f32 = 6.0;
pub const LUNGE_PAD: f32 = 0.5;
pub const LUNGE_SPEED: f32 = 18.0;
pub const LUNGE_MS: u32 = 400;
/// Feet to feet, across the ground, and up or down within a body's height.
pub const STRIKE_R: f32 = 1.2;
/// How hard a clash throws each back.
pub const CLASH_PUSH: f32 = 8.0;

pub const GREN_DMG: i32 = 150;
pub const GREN_RADIUS: f32 = 5.0;
pub const GREN_FULL: f32 = 1.0;
pub const GREN_FUSE_MS: u32 = 1800;
pub const GREN_SPEED: f32 = 16.0;
pub const GREN_START: u8 = 2;
pub const GREN_MAX: u8 = 4;
pub const GREN_COOL_MS: u32 = 600;
/// Speed kept along the normal and the surface in a bounce.
pub const GREN_BOUNCE: f32 = 0.45;
pub const GREN_SLIDE: f32 = 0.7;
/// Velocity a blast gives at full strength, away from its centre.
pub const BLAST_PUSH: f32 = 9.0;

pub const COUNTDOWN_MS: u32 = 3000;
/// Range of a pad: within this in x and z, and 1.5 m in height.
pub const PAD_REACH: f32 = 1.0;

// Vehicles (sim/src/vehicle.rs): one kind so far, a light four-wheeler for
// a driver and a passenger who shoots.
/// Vehicles on a map at once.
pub const MAX_VEHICLES: usize = 8;
/// Half the hull's length and width, and its height above the ground.
pub const VEH_HALF_L: f32 = 2.2;
pub const VEH_HALF_W: f32 = 1.1;
pub const VEH_H: f32 = 1.5;
/// The hull meets boxes as two circles this far fore and aft of its middle.
pub const VEH_HULL_OFF: f32 = 1.1;
pub const VEH_HULL_R: f32 = 1.1;
/// Driving, per second: throttle, braking against the way it rolls, top
/// speed forward and back, rolling drag (a fraction of speed), sideways
/// grip (likewise), and turning: radians at full steer once past
/// `VEH_TURN_FULL` m/s.
pub const VEH_ACCEL: f32 = 13.0;
pub const VEH_BRAKE: f32 = 24.0;
pub const VEH_TOP: f32 = 21.0;
pub const VEH_REVERSE: f32 = 8.0;
pub const VEH_ROLL: f32 = 0.35;
pub const VEH_GRIP: f32 = 7.0;
pub const VEH_TURN: f32 = 1.9;
pub const VEH_TURN_FULL: f32 = 6.0;
/// Slower than this, an empty vehicle on the ground stands still.
pub const VEH_PARK: f32 = 0.5;
/// The share of a slope's pull across the heading the tyres let through.
pub const VEH_SIDE_SLIP: f32 = 0.15;
/// It drives onto a ledge this high and stays on the ground over a drop
/// this deep.
pub const VEH_STEP: f32 = 0.6;
pub const VEH_SNAP: f32 = 0.4;
/// Hit points (in `PT`s, like a fighter's).
pub const VEH_HEALTH: i32 = 900 * PT;
/// How near a fighter must stand to get in.
pub const ENTER_R: f32 = 3.0;
/// Faster than this, it runs a fighter down; past `SPLAT_KILL` the hit is
/// lethal, below it scaled.
pub const SPLAT_SPEED: f32 = 5.0;
pub const SPLAT_KILL: f32 = 11.0;
/// How much of a blast's push a vehicle takes.
pub const VEH_PUSH_K: f32 = 0.3;
