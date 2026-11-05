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
/// Weapon and power-up pads on a map (the Range has one of every gun
/// and power-up).
pub const MAX_PADS: usize = 24;
/// Events one tick may raise; past it they are dropped and counted.
pub const MAX_EVENTS: usize = 128;
/// Boxes and solids in a map, together (`tools/maps/compile.mjs` holds
/// maps to it).
pub const MAX_MAP_BOXES: usize = 512;
/// The most knots a profile solid has (crate::profile).
pub const MAX_KNOTS: usize = 32;
/// Ticks of pose history for lag compensation (250 ms at 128 Hz).
pub const REWIND_TICKS: usize = 32;

// Movement: Quake's model (Fireplace's `PHYS`), restated per second, at
// Halo 2's speeds and jump. Its world unit is 10 ft (3.048 m), the scale
// the guns are in: 2.25 wu/s ahead, 2.0 backing up and to the side (the
// Editing Kit's player metrics); between, on the ellipse
// (`movement::step`).
pub const RUN: f32 = 6.86;
pub const RUN_BACK: f32 = 6.10;
pub const RUN_SIDE: f32 = 6.10;
pub const CROUCH_RUN: f32 = 3.5;
/// Below this speed friction acts as if moving at it, so a body stops.
pub const STOP: f32 = 2.5;
pub const FRICTION: f32 = 6.0;
pub const ACCEL: f32 = 10.0;
pub const AIR_ACCEL: f32 = 10.0;
/// Quake's air wish-speed cap: air strafing works, air running doesn't.
pub const AIR_CAP: f32 = 1.0;
/// Halo 2's jump in proportion to our body (it jumps half again its own
/// height and is 1.4 s in the air: Halo 3's measured, the same jump
/// velocity): an apex of v²/2g = 2.76 m, 2v/g = 1.40 s up and down.
/// Grenades fall at it whole (Halo's "100% gravity").
pub const GRAVITY: f32 = 11.3;
pub const JUMP_SPEED: f32 = 7.9;
pub const VMAX: f32 = 60.0;

// The body: a box for walking, a cylinder and a sphere for being shot.
pub const BODY_R: f32 = 0.35;
pub const BODY_H: f32 = 1.8;
pub const CROUCH_H: f32 = 1.3;
pub const EYE: f32 = 1.6;
pub const CROUCH_EYE: f32 = 1.1;
/// Crouching in the air tucks the legs up this far (Halo's crouch jump:
/// the head stays, the feet rise, a higher ledge is reached), and letting
/// go puts them back down where there is room.
pub const CROUCH_LIFT: f32 = BODY_H - CROUCH_H;
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
/// Where a near miss is bent to (bullet magnetism, `World::magnet`): the
/// chest, never the head.
pub const CHEST_Y: f32 = 1.0;
pub const CHEST_Y_CROUCH: f32 = 0.65;
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

// Power-ups (arms::OVERSHIELD, arms::CAMO), on pads, taken by walking on.
/// The overshield: shields climb to three times full (Halo 2's), at
/// `OVERSHIELD_RATE` points a second from wherever they were (health whole
/// at once), then drain at `OVERSHIELD_DRAIN` back to full: 40 s from the
/// top. Above full, shields never recharge; below it they do as ever.
pub const OVERSHIELD_MAX: i32 = 3 * SHIELD_MAX;
pub const OVERSHIELD_RATE: i32 = 100;
pub const OVERSHIELD_DRAIN: i32 = 5;
/// Active camouflage lasts this long. Moving faster than `CAMO_WALK` shows
/// a body, fully at `CAMO_RUN` and over; firing, a throw, a blow or taking
/// a hit shows it whole for `CAMO_FLARE_MS`, fading.
pub const CAMO_MS: u32 = 45_000;
pub const CAMO_WALK: f32 = 2.0;
pub const CAMO_RUN: f32 = 9.0;
pub const CAMO_FLARE_MS: u32 = 1200;
/// How hidden a camouflaged body still standing is: never all the way.
pub const CAMO_BEST: f32 = 0.92;
/// A bot doesn't make out a camouflaged enemy hidden past
/// `CAMO_BOT_HIDDEN` farther than `CAMO_BOT_M`.
pub const CAMO_BOT_HIDDEN: f32 = 0.6;
pub const CAMO_BOT_M: f32 = 12.0;
/// Where someone comes back (`World::respawn`): of the spawns their side
/// may use, the one whose nearest living enemy is farthest, counted only
/// to `SPAWN_FAR_M` (past it, far is far); one a living enemy can see
/// counts `SPAWN_SEEN_M` less, one with a living teammate within
/// `SPAWN_MATE_R` counts `SPAWN_MATE_M` more (Halo's: back beside your
/// side); and each draws up to `SPAWN_JITTER_M` more, so the same spawn
/// isn't a certainty to be camped.
pub const SPAWN_FAR_M: f32 = 30.0;
pub const SPAWN_SEEN_M: f32 = 25.0;
pub const SPAWN_MATE_R: f32 = 15.0;
pub const SPAWN_MATE_M: f32 = 10.0;
pub const SPAWN_JITTER_M: f32 = 3.0;

pub const MELEE_DMG: i32 = 70;
pub const MELEE_RANGE: f32 = 2.0;
pub const MELEE_COOL_MS: u32 = 800;
/// Halo 2's melee lunge: a melee with the aim (the body grown by
/// `MELEE_LUNGE_PAD`) on someone past `MELEE_RANGE` but within
/// `MELEE_LUNGE_RANGE` drives the body at them at `MELEE_LUNGE_SPEED`, for
/// up to `MELEE_LUNGE_MS`, and lands the blow on reaching `STRIKE_R`.
pub const MELEE_LUNGE_RANGE: f32 = 3.0;
pub const MELEE_LUNGE_PAD: f32 = 0.4;
pub const MELEE_LUNGE_SPEED: f32 = 12.0;
pub const MELEE_LUNGE_MS: u32 = 250;
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

/// A homing round (a shard, a charged bolt) locks on whoever its aim
/// passes this near, at the shot.
pub const HOMING_PAD: f32 = 1.0;
/// Shards fall out of a fighter this long after the last went in.
pub const NEEDLE_KEEP_MS: u32 = 1500;
/// What a charged bolt does to a hull.
pub const CHARGE_VEH_DMG: i32 = 60;

/// A frag, as Halo 2's multiplayer throws it (Halopedia's H2 weapon data,
/// at 3.048 m a world unit): 160 out to 0.5 wu, down to 50 at 1.75 wu;
/// thrown at 10 wu/s; off `GREN_FUSE_MS` after it first comes down on a
/// floor (not a wall), never sooner than `GREN_ARM_MS` from the throw, and
/// at `GREN_LIFE_MS` if it never does.
pub const GREN_DMG: i32 = 160;
pub const GREN_RADIUS: f32 = 5.3;
pub const GREN_FULL: f32 = 1.5;
pub const GREN_EDGE: i32 = 50;
pub const GREN_FUSE_MS: u32 = 500;
pub const GREN_ARM_MS: u32 = 1300;
pub const GREN_LIFE_MS: u32 = 5000;
pub const GREN_SPEED: f32 = 30.5;
pub const GREN_START: u8 = 2;
/// Sticky grenades at a spawn (at most `GREN_MAX` too). Halo 2 gives
/// plasma grenades from the map; ours has no grenade pickups yet.
pub const STICKY_START: u8 = 1;
/// A sticky (Halo 2's plasma grenade in multiplayer): it sticks to the
/// first body or hull it touches (not to the map: it skips off that), and
/// goes off `STICKY_FUSE_MS` after it sticks or first lands, never before
/// `STICKY_ARM_MS` from the throw. Its blast kills to `STICKY_FULL`; the
/// one it is stuck to dies whatever.
pub const STICKY_FUSE_MS: u32 = 900;
pub const STICKY_ARM_MS: u32 = 1300;
pub const STICKY_DMG: i32 = 200;
pub const STICKY_RADIUS: f32 = 5.3;
pub const STICKY_FULL: f32 = 1.5;
/// What the sticky's blast still does at its rim (Halo 2's, as the frag's).
pub const STICKY_EDGE: i32 = 50;
/// Dual wielding: both hands reload together, this much slower.
pub const DUAL_RELOAD_K: f32 = 1.5;
pub const GREN_MAX: u8 = 4;
pub const GREN_COOL_MS: u32 = 600;
/// Speed kept along the normal and the surface in a bounce.
pub const GREN_BOUNCE: f32 = 0.45;
pub const GREN_SLIDE: f32 = 0.7;
/// Velocity a blast gives at full strength, away from its centre.
pub const BLAST_PUSH: f32 = 9.0;
/// A vehicle going up: its blast.
pub const WRECK_DMG: i32 = 150;
pub const WRECK_RADIUS: f32 = 5.0;
pub const WRECK_FULL: f32 = 1.0;

pub const COUNTDOWN_MS: u32 = 3000;
/// Range of a pad: within this in x and z, and 1.5 m in height.
pub const PAD_REACH: f32 = 1.0;

// What the dead (and a swap) leave on the floor: the gun in hand, the left
// hand's, and the grenades carried (`World::spill`).
/// Drops on the floor at once; a new one past this takes the oldest's slot.
pub const MAX_DROPS: usize = 24;
/// How long a drop lies before it goes.
pub const DROP_MS: u32 = 30_000;
/// A drop is in reach as a pad is: within this in x and z, 1.5 m in height.
pub const DROP_REACH: f32 = 1.0;
/// The left gun and the grenades fall this far aside of the gun in hand,
/// inside the body's own footprint (so never in a wall).
pub const DROP_APART: f32 = 0.3;
/// A blast throws a drop as it throws a body, by this share of the push,
/// and lifts it this fast at full strength, m/s, so one lying beside a
/// frag pops up rather than skating (a rocket at a metre: about 8 m).
pub const DROP_PUSH_K: f32 = 0.7;
pub const DROP_LIFT: f32 = 4.5;
/// Where a blast meets a drop: this far over the floor it lies on.
pub const DROP_MID: f32 = 0.15;
/// A drop's bounce: the speed into a surface it keeps, and the speed along
/// a floor it keeps each tick it skids (a skid of a tenth of a second).
/// Below this speed on a floor it lies still.
pub const DROP_BOUNCE: f32 = 0.25;
pub const DROP_SLIDE: f32 = 0.92;
pub const DROP_SETTLE: f32 = 0.4;
/// What the dead's guns leave their hands with: this share of the body's
/// velocity, and this fast up.
pub const DROP_CARRY: f32 = 0.6;
pub const DROP_TOSS: f32 = 1.5;
/// A drop below this height has fallen out of the world.
pub const DROP_LOST_Y: f32 = -50.0;

// Vehicles (sim/src/vehicle.rs): one kind so far, a truck for a driver in
// its cab and a passenger who shoots from its bed. Its box is the model the
// client draws (`models/truck.glb`, `look/truck.rs`).
/// Vehicles on a map at once.
pub const MAX_VEHICLES: usize = 8;
/// Half the hull's length and width, and its height above the ground.
pub const VEH_HALF_L: f32 = 2.86;
pub const VEH_HALF_W: f32 = 1.1;
pub const VEH_H: f32 = 1.84;
/// What of it stops a shot (`Vehicle::ray`): its steel, never its glass.
/// The body to the windows' sill, its whole length; and behind the cab
/// (from this far aft of its middle), the bed's sides to their rail. A
/// driver's head and shoulders are over the one, behind glass; a
/// passenger's chest and head over the other.
pub const VEH_SILL: f32 = 1.25;
pub const VEH_BED_X: f32 = -0.84;
pub const VEH_BED_H: f32 = 1.45;
/// The hull meets boxes as three circles: one at its middle, and one this
/// far fore and aft of it (to its nose and its tail).
pub const VEH_HULL_OFF: f32 = 1.76;
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
/// How near its middle a fighter must stand to get in: at its nose or its
/// tail it still can.
pub const ENTER_R: f32 = 3.6;
/// Faster than this, it runs a fighter down; past `SPLAT_KILL` the hit is
/// lethal, below it scaled.
pub const SPLAT_SPEED: f32 = 5.0;
pub const SPLAT_KILL: f32 = 11.0;
/// How much of a blast's push a vehicle takes.
pub const VEH_PUSH_K: f32 = 0.3;

// Capture the Flag.
/// How near (flat) a fighter must come to a flag to take, return or score
/// it, and how far above or below.
pub const FLAG_REACH: f32 = 1.2;
pub const FLAG_RISE: f32 = 2.0;
/// A flag left on the floor goes home on its own after this.
pub const FLAG_RETURN_MS: u32 = 30_000;
/// Nobody takes a flag in its first second on the floor, so a carrier who
/// lets go doesn't pick it straight back up.
pub const FLAG_REGRAB_MS: u32 = 1000;
