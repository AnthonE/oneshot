//! The fx stage (`?pose=1&fx=<name>`, `tools/fx.sh`, the `fx` skill): one
//! effect staged alone, fired at once in front of a dark card and a light
//! card high above the map, on a clock that steps 1/60 s and holds where the
//! page asks (`window.__fxStep`), so `tools/fxstrip.mjs` shoots it frame by
//! frame. What it fires is the game's own (`Fx::impact`, `shots::flash`,
//! `blast::blast`, `bodies::pop`), so a strip is what the game throws.
//!
//! `knobs=Sparks.speed=8,16;…` retunes layers (`table::set_knobs`) and
//! `seed=N` rolls the pools differently; `window.__fx` says where it stands.
//! The contract is the shared crate's (`fx::stage`).

use ::fx::stage::{Clock, Framing, Tick};
use bevy::prelude::*;
use bevy::time::{TimeSystems, TimeUpdateStrategy};
use sim::arms::{
    AR, BEAM_RIFLE, BR, CANNON, CARBINE, ION_PISTOL, ION_RIFLE, LAUNCHER, MAGNUM, ROCKET, SHARDS, SHOTGUN, SMG, SNIPER,
};
use sim::events::Cause;

use super::table::{self, Matter, Weapon};
use super::{blast, bodies, shots, Fx, Scene};
use crate::{Eye, LinkRes};

/// Where the stage stands: high over the map, so nothing of it is in view.
const ORIGIN: Vec3 = Vec3::new(0.0, 80.0, 0.0);

/// The cards' sRGB greys, tuned under our exposure and tonemap until the
/// plate reads the stage's ~25 and ~185 (`fxstrip.mjs` says `plate … ok`).
const CARDS: [f32; 2] = [::fx::stage::DARK_SRGB, 0.86];

/// What an effect is fired by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fire {
    /// A blow: `Fx::impact`.
    Hit(Weapon, Matter),
    /// A gun's muzzle: `shots::flash` with its look (`table::gun`).
    Muzzle(u8),
    /// An explosion: `blast::blast` with its cause (a grenade, a gun's id).
    Blast(u16),
    /// A shield breaking on a fighter: `bodies::pop`.
    Pop,
    /// A vehicle blowing up and burning: `blast::wreck`.
    Wreck,
}

/// One effect the stage can fire.
#[derive(Clone, Debug)]
pub struct Staged {
    pub name: String,
    pub what: String,
    /// Roughly how far it reaches, metres: the stage frames it by this.
    pub radius: f32,
    /// How long it lasts, seconds: a strip's frames span this.
    pub span_s: f32,
    pub fire: Fire,
}

const WEAPONS: [(Weapon, &str); 7] = [
    (Weapon::Bullet, "bullet"),
    (Weapon::Pellet, "pellet"),
    (Weapon::Sniper, "sniper"),
    (Weapon::Melee, "melee"),
    (Weapon::Blade, "blade"),
    (Weapon::Carbine, "carbine"),
    (Weapon::Beam, "beam"),
];

const MATTERS: [(Matter, &str); 6] = [
    (Matter::Concrete, "concrete"),
    (Matter::Metal, "metal"),
    (Matter::Dirt, "dirt"),
    (Matter::Shield, "shield"),
    (Matter::Flesh, "flesh"),
    (Matter::Hull, "hull"),
];

/// Every effect the stage can fire, in a stable order.
pub fn list() -> Vec<Staged> {
    let mut out = Vec::new();
    let s = |name: &str, what: &str, radius: f32, span_s: f32, fire: Fire| Staged {
        name: name.into(),
        what: what.into(),
        radius,
        span_s,
        fire,
    };
    for (w, wn) in WEAPONS {
        for (m, mn) in MATTERS {
            out.push(s(
                &format!("hit-{wn}-{mn}"),
                &format!("{wn} on {mn}"),
                0.35,
                1.2,
                Fire::Hit(w, m),
            ));
        }
    }
    for (arm, n) in [
        (AR, "rifle"),
        (BR, "br"),
        (MAGNUM, "magnum"),
        (SMG, "smg"),
        (SHOTGUN, "shotgun"),
        (SNIPER, "sniper"),
        (ROCKET, "rocket"),
        (LAUNCHER, "launcher"),
        (CARBINE, "carbine"),
        (BEAM_RIFLE, "beam"),
        (ION_RIFLE, "ion"),
        (ION_PISTOL, "pistol"),
        (SHARDS, "shards"),
        (CANNON, "cannon"),
    ] {
        out.push(s(
            &format!("muzzle-{n}"),
            &format!("the {n}'s muzzle flash"),
            0.3,
            0.6,
            Fire::Muzzle(arm),
        ));
    }
    out.push(s(
        "grenade",
        "a frag going off",
        2.2,
        2.5,
        Fire::Blast(Cause::Grenade as u16),
    ));
    out.push(s("rocket", "a rocket going off", 2.4, 2.5, Fire::Blast(ROCKET as u16)));
    out.push(s(
        "sticky",
        "a sticky going off",
        2.0,
        1.6,
        Fire::Blast(Cause::Sticky as u16),
    ));
    out.push(s("wreck", "a vehicle blowing up and burning", 3.2, 4.0, Fire::Wreck));
    out.push(s(
        "cannon",
        "the ion cannon's ball bursting",
        1.6,
        1.2,
        Fire::Blast(sim::arms::CANNON as u16),
    ));
    out.push(s(
        "shards",
        "the SHARD GUN's shards going off together",
        1.2,
        1.2,
        Fire::Blast(sim::arms::SHARDS as u16),
    ));
    out.push(s("shield-pop", "a shield breaking on a fighter", 1.2, 1.0, Fire::Pop));
    out
}

pub fn find(name: &str) -> Option<Staged> {
    list().into_iter().find(|s| s.name == name)
}

/// The stage as the page asked for it.
#[derive(Resource)]
pub struct Stage {
    pub staged: Staged,
    pub framing: Framing,
    pub clock: Clock,
    pub tick: Tick,
    pub knobs: String,
    pub seed: u32,
}

impl Stage {
    /// From the page's query (`fx=hit-bullet-metal&knobs=…&seed=3`), if it
    /// names an effect. A bad name or knob is an error on the console.
    pub fn parse(q: &str) -> Option<Stage> {
        let get = |k: &str| {
            q.trim_start_matches('?')
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .find(|(key, _)| *key == k)
                .map(|(_, v)| decode(v))
        };
        let name = get("fx")?;
        let Some(staged) = find(&name) else {
            error!("fx stage: no effect `{name}`");
            return None;
        };
        let knobs = get("knobs").unwrap_or_default();
        if !knobs.is_empty() {
            if let Err(e) = table::set_knobs(&knobs) {
                error!("fx stage: {e}");
            }
        }
        Some(Stage {
            framing: Framing::new(staged.radius),
            // A person watching (no shooter driving it) sees it again and
            // again, with a breath between.
            clock: Clock::looping((staged.span_s * ::fx::stage::HZ as f32) as u32 + 30),
            tick: Tick {
                dt: 0.0,
                step: 0,
                fire: false,
                clear: false,
            },
            staged,
            knobs,
            seed: get("seed").and_then(|s| s.parse().ok()).unwrap_or(0),
        })
    }
}

/// `%2C` and friends, as a page's query carries them.
fn decode(v: &str) -> String {
    let b = v.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = v.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b[i], hex) {
            (b'%', Some(x)) => {
                out.push(x);
                i += 3;
                continue;
            }
            (b'+', _) => out.push(b' '),
            (c, _) => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The stage's systems, on an app already built for `?pose`.
pub fn install(app: &mut App, stage: Stage) {
    if let Some(mut p) = app.world_mut().get_resource_mut::<crate::pose::Pose>() {
        p.2.fighter = false;
    }
    app.insert_resource(stage)
        .insert_resource(TimeUpdateStrategy::ManualDuration(core::time::Duration::ZERO))
        .add_systems(Startup, cards)
        .add_systems(First, clock.before(TimeSystems))
        .add_systems(Update, (fire.before(super::draw), aim.after(crate::pose::aim)));
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, expose.after(super::draw));
}

/// The step the page asks for, if a shooter drives it.
fn want() -> Option<u32> {
    #[cfg(target_arch = "wasm32")]
    {
        let w = web_sys::window()?;
        let v = js_sys::Reflect::get(&w, &wasm_bindgen::JsValue::from_str("__fxStep")).ok()?;
        v.as_f64().map(|f| f.max(0.0) as u32)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Before time moves: this frame is a step or a hold.
fn clock(mut stage: ResMut<Stage>, mut strategy: ResMut<TimeUpdateStrategy>) {
    let tick = stage.clock.frame(want());
    stage.tick = tick;
    *strategy = TimeUpdateStrategy::ManualDuration(core::time::Duration::from_secs_f32(tick.dt));
}

/// The two cards, behind where the effects fire.
fn cards(
    stage: Res<Stage>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    for (c, g) in stage.framing.cards.iter().zip(CARDS) {
        commands.spawn((
            Mesh3d(quad.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(g, g, g),
                unlit: true,
                ..default()
            })),
            Transform::from_translation(ORIGIN + Vec3::from(c.centre) - Vec3::Z * 0.002).with_scale(Vec3::new(
                c.half[0] * 2.0,
                c.half[1] * 2.0,
                1.0,
            )),
        ));
    }
}

/// The camera the stage frames, over the pose's.
fn aim(stage: Res<Stage>, mut cam: Query<(&mut Transform, &mut Projection), With<Eye>>) {
    let f = &stage.framing;
    if let Ok((mut t, mut proj)) = cam.single_mut() {
        *t = Transform::from_translation(ORIGIN + Vec3::from(f.eye)).looking_at(ORIGIN + Vec3::from(f.look), Vec3::Y);
        if let Projection::Perspective(pp) = &mut *proj {
            pp.fov = f.fov_y;
        }
    }
}

/// Fire on step 1 at both anchors; clear when the loop starts over.
fn fire(link: NonSend<LinkRes>, stage: Res<Stage>, time: Res<Time>, mut fx: ResMut<Fx>) {
    let tick = stage.tick;
    if tick.clear || (tick.fire && tick.step == 1) {
        fx.glow.clear();
        fx.soft.clear();
        fx.cloud.clear();
        fx.boom.clear();
        fx.chips.clear();
        fx.shocks = [super::shell::Shock::default(); super::shell::SHELLS];
        fx.glow.seed(stage.seed);
        fx.soft.seed(stage.seed.wrapping_mul(0x9E37_79B9));
        fx.cloud.seed(stage.seed.wrapping_mul(0x85EB_CA6B));
        fx.boom.seed(stage.seed.wrapping_mul(0xC2B2_AE35));
        fx.chips.seed(stage.seed);
    }
    if !tick.fire {
        return;
    }
    let core = &link.0.session().core;
    let scene = Scene {
        map: &core.map,
        others: &[],
        vehicles: &[],
        me: core.slot,
        aim: Vec3::from(stage.framing.dir),
        t: time.elapsed_secs(),
    };
    let f = &stage.framing;
    let dir = Vec3::from(f.dir);
    for a in f.anchors {
        let at = ORIGIN + Vec3::from(a);
        match stage.staged.fire {
            // The mark flat on the card's plane: no map is drawn here.
            Fire::Hit(w, m) => {
                fx.impact(w, m, at, dir, f32::NEG_INFINITY);
                if let Some((k, size)) = super::marks::kind(w, m) {
                    fx.marks.place(k, size, at, dir, m);
                }
            }
            Fire::Muzzle(arm) => shots::flash(&mut fx, at, dir, &table::gun(arm)),
            Fire::Blast(c) => blast::blast(&mut fx, &scene, at, c, false),
            Fire::Wreck => blast::wreck(&mut fx, &scene, at - Vec3::Y * 0.8),
            Fire::Pop => bodies::pop(&mut fx, at - Vec3::Y * 1.0),
        }
    }
}

/// `window.__fx`: where the stage stands, for `fxstrip.mjs`.
#[cfg(target_arch = "wasm32")]
fn expose(stage: Res<Stage>, fx: Res<Fx>) {
    use wasm_bindgen::JsValue;
    let Some(w) = web_sys::window() else {
        return;
    };
    let o = js_sys::Object::new();
    let set = |k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(&o, &JsValue::from_str(k), &v);
    };
    set("game", "oneshot".into());
    set("name", stage.staged.name.as_str().into());
    set("what", stage.staged.what.as_str().into());
    set("span", stage.staged.span_s.into());
    set("step", stage.clock.step().into());
    set("ready", stage.clock.ready().into());
    set("held", stage.clock.held().into());
    set("seed", stage.seed.into());
    set("knobs", stage.knobs.as_str().into());
    set(
        "live",
        js_sys::Array::of4(
            &(fx.glow.live() as u32).into(),
            &(fx.soft.live() as u32).into(),
            &(fx.cloud.live() as u32).into(),
            &(fx.boom.live() as u32).into(),
        )
        .into(),
    );
    let recipes = js_sys::Object::new();
    for l in table::Layer::ALL {
        let _ = js_sys::Reflect::set(
            &recipes,
            &JsValue::from_str(l.name()),
            &table::recipe(l).to_string().into(),
        );
    }
    set("recipes", recipes.into());
    let _ = js_sys::Reflect::set(&w, &JsValue::from_str("__fx"), &o);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_names_an_effect_and_its_knobs() {
        assert!(Stage::parse("?yaw=1").is_none());
        assert!(Stage::parse("fx=nothing-here").is_none());
        let s = Stage::parse("?pose=1&fx=hit-bullet-metal&seed=3").expect("a stage");
        assert_eq!(s.staged.fire, Fire::Hit(Weapon::Bullet, Matter::Metal));
        assert_eq!(s.seed, 3);
        assert_eq!(
            decode("Sparks.speed%3D8%2C16%3BFlash.life%3D0.04"),
            "Sparks.speed=8,16;Flash.life=0.04"
        );
        let names: Vec<_> = list().into_iter().map(|s| s.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "names are unique");
    }
}
