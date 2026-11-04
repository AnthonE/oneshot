//! Whether what the camera sees is all there yet, and the screen a match
//! opens under while it isn't: after Gates' `client/src/render/loading.rs`
//! (MIT), trimmed to an arena's wait.
//!
//! **The wait is on state, never on a clock.** [`Settle`] counts what a
//! level is made of: built for this link (`front::LinkGen`), every ground,
//! box and grass part in its textures, every model in and its warm draw
//! retired (`look/prewarm.rs`), and no render pipeline still compiling
//! (`look/pipelines.rs`), held [`DONE_FRAMES`] because the pipeline count is
//! the render world's and arrives a frame late. The title's curtain
//! (`front.rs`), this overlay, the `--shot` capture (`look/shot.rs`) and the
//! browser's `__oneshot.ready` all read it.
//!
//! **The overlay never holds the match.** The match is live the moment the
//! welcome lands (`front::poll`): the session runs, the controls send, Esc
//! opens the menu over it. The overlay only covers the frames a level spends
//! arriving (pieces in grey, models popping in, on the desktop a skipped
//! draw while its pipeline compiles), and it lifts after [`LOADING_GUARD_S`]
//! whatever is still missing, so a slow machine sees a level dressing in
//! rather than a wall.
//!
//! The same on the desktop and in a browser: one camera, so the overlay is
//! drawn with the UI pipeline every screen before it already built (Gates
//! needed a camera of its own for its loading screen only because its world
//! camera's UI pass was compiled first there).

use bevy::asset::LoadState;
use bevy::prelude::*;

use crate::front::{Front, InMatch, LinkGen};
use crate::level::{BoxLook, Dressed, GrassPart, GroundMesh, LevelPart};
use crate::look::pipelines::PipelineBacklog;
use crate::look::prewarm::Warming;
use crate::{ui, LinkRes};

/// Frames everything must read done before it counts.
pub const DONE_FRAMES: u32 = 3;
/// Frames with nothing but pipelines left and their count not moving before
/// it counts anyway: a shader that never loads is a bug, but it mustn't
/// keep a screen up.
pub const PIPELINE_STALL_FRAMES: u32 = 300;
/// The overlay over a new match lifts after this many seconds whatever is
/// still arriving.
pub const LOADING_GUARD_S: f64 = 5.0;
/// How long the overlay takes to fade into the match.
const FADE_S: f32 = 0.35;
/// The bar, in pixels at 1280 by 720 (the title's panel is 380 wide).
const BAR_W: f32 = 520.0;
const BAR_H: f32 = 8.0;
/// The sweep along the bar while something has no count to show (shaders
/// compiling): its width as a share of the bar, and seconds a pass. The
/// page's splash bar moves the same way (`web/index.html`).
const SWEEP_W: f32 = 22.0;
const SWEEP_S: f32 = 1.2;
/// Above the HUD, under the Esc menu and the settings (both 10).
const Z: i32 = 9;

/// The clear colour once a level is spawned: where a sky isn't drawn yet
/// (its pipeline still compiling) it is a sky's colour, not a hole.
pub const SKY_CLEAR: Color = Color::srgb(0.62, 0.77, 0.91);

/// What a level is made of, counted this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    /// The level is spawned for this link.
    pub built: bool,
    /// Its parts in their textures, of all its parts.
    pub parts: (usize, usize),
    /// Every model loaded (or failed for good).
    pub models: bool,
    /// Warm draws still out.
    pub warming: usize,
    /// Render pipelines still compiling.
    pub pipelines: u32,
}

impl Progress {
    /// Everything but the pipelines.
    pub fn built(&self) -> bool {
        self.built && self.parts.0 >= self.parts.1 && self.models && self.warming == 0
    }

    pub fn done(&self) -> bool {
        self.built() && self.pipelines == 0
    }

    /// The bar: four equal steps, the parts' counted as they dress.
    pub fn fraction(&self) -> f32 {
        let built = if self.built { 1.0 } else { 0.0 };
        let parts = match self.parts {
            _ if !self.built => 0.0,
            (_, 0) => 1.0,
            (d, n) => (d as f32 / n as f32).min(1.0),
        };
        let models = match (self.models, self.warming) {
            (false, _) => 0.0,
            (true, 0) => 1.0,
            _ => 0.5,
        };
        let shaders = if self.built() && self.pipelines == 0 { 1.0 } else { 0.0 };
        (built + parts + models + shaders) / 4.0
    }

    /// The line over the bar.
    pub fn stage(&self) -> String {
        if !self.built {
            "BUILDING THE LEVEL".into()
        } else if self.parts.0 < self.parts.1 {
            format!("DRESSING THE LEVEL · {} OF {}", self.parts.0, self.parts.1)
        } else if !self.models || self.warming > 0 {
            "WARMING THE MODELS".into()
        } else if self.pipelines > 0 {
            format!("COMPILING SHADERS · {}", self.pipelines)
        } else {
            "READY".into()
        }
    }
}

/// Whether the current link's level is all there.
#[derive(Resource, Default)]
pub struct Settle {
    /// The link (`LinkGen`) these counts are about.
    gen: Option<u32>,
    /// When that link came (real seconds).
    since: f64,
    held: u32,
    /// The pipeline count last frame, and frames it hasn't moved.
    stall: (u32, u32),
    ready: bool,
    /// Ready, and nothing over it: no curtain, no loading overlay
    /// ([`shown`]).
    shown: bool,
    pub now: Progress,
}

impl Settle {
    /// Everything there (latched for the link: a pipeline compiled later,
    /// for a rocket's first flight, doesn't bring a screen back).
    pub fn ready(&self) -> bool {
        self.ready
    }

    /// Ready, and on screen with nothing over it.
    pub fn shown(&self) -> bool {
        self.shown
    }

    pub fn gen(&self) -> Option<u32> {
        self.gen
    }

    /// Seconds since this link came.
    pub fn age(&self, now: f64) -> f64 {
        now - self.since
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Settle>()
        .add_plugins(crate::look::pipelines::plugin)
        .add_systems(
            Update,
            (settle, start, update, fade)
                .chain()
                .after(crate::level::dress_level)
                .after(crate::look::prewarm::retire),
        )
        .add_systems(PostUpdate, shown);
}

/// The scenes every level draws with.
fn models_in(models: Option<&crate::look::guns::Models>, assets: &AssetServer) -> bool {
    let Some(m) = models else {
        return false;
    };
    let settled = |h: &Handle<Scene>| matches!(assets.load_state(h), LoadState::Loaded | LoadState::Failed(_));
    m.guns
        .iter()
        .flatten()
        .chain([&m.rocket, &m.frag, &m.body_scene])
        .all(settled)
}

/// A level's parts that wait for their textures, and whether each has them.
type Parts<'w, 's> =
    Query<'w, 's, Has<Dressed>, (With<LevelPart>, Or<(With<GroundMesh>, With<BoxLook>, With<GrassPart>)>)>;

/// This frame's counts, and whether they add up to a level.
#[allow(clippy::too_many_arguments)]
fn settle(
    mut s: ResMut<Settle>,
    built: Res<crate::Built>,
    gen: Option<Res<LinkGen>>,
    backlog: Res<PipelineBacklog>,
    models: Option<Res<crate::look::guns::Models>>,
    assets: Res<AssetServer>,
    parts: Parts,
    warming: Query<(), With<Warming>>,
    time: Res<Time<Real>>,
    mut clear: ResMut<ClearColor>,
) {
    let now = time.elapsed_secs_f64();
    let g = gen.map_or(0, |g| g.0);
    if s.gen != Some(g) {
        *s = Settle {
            gen: Some(g),
            since: now,
            ..default()
        };
    }
    let p = Progress {
        built: built.gen() == Some(g),
        parts: parts
            .iter()
            .fold((0, 0), |(d, n), dressed| (d + dressed as usize, n + 1)),
        models: models_in(models.as_deref(), &assets),
        warming: warming.iter().count(),
        pipelines: backlog.pending(),
    };
    s.now = p;
    if p.built && clear.0 != SKY_CLEAR {
        clear.0 = SKY_CLEAR;
    }
    if s.ready {
        return;
    }
    s.held = if p.done() { s.held + 1 } else { 0 };
    let (last, frames) = s.stall;
    s.stall = (
        p.pipelines,
        if p.built() && p.pipelines > 0 && p.pipelines == last {
            frames + 1
        } else {
            0
        },
    );
    let stalled = s.stall.1 >= PIPELINE_STALL_FRAMES;
    if stalled {
        warn!(
            "oneshot: {} pipelines still compiling after {PIPELINE_STALL_FRAMES} frames; counting the level as there",
            p.pipelines
        );
    }
    if s.held >= DONE_FRAMES || stalled {
        s.ready = true;
        info!(
            "oneshot: the level is there after {:.1} s ({} parts, link {g})",
            s.age(now),
            p.parts.1
        );
    }
}

/// The overlay over a new match.
#[derive(Component)]
pub struct LoadingRoot {
    gen: u32,
}

#[derive(Component)]
struct Bar;

#[derive(Component)]
struct Sweep;

#[derive(Component)]
struct Stage;

/// On its way out: alpha from 1 to 0 over [`FADE_S`], then despawned.
#[derive(Component)]
struct Fading {
    t: f32,
    /// The factor applied so far, so each frame scales by the ratio.
    k: f32,
}

/// A new match (a link swapped in by the welcome): the overlay over it,
/// until its level is there. Off a match, none.
fn start(
    mut commands: Commands,
    in_match: Option<Res<InMatch>>,
    gen: Option<Res<LinkGen>>,
    front: Option<Res<Front>>,
    link: NonSend<LinkRes>,
    roots: Query<(Entity, &LoadingRoot)>,
    mut shown_for: Local<Option<u32>>,
) {
    let (Some(in_match), Some(gen)) = (in_match, gen) else {
        return;
    };
    if !in_match.0 {
        *shown_for = None;
        for (e, _) in &roots {
            commands.entity(e).despawn();
        }
        return;
    }
    if *shown_for == Some(gen.0) {
        return;
    }
    *shown_for = Some(gen.0);
    for (e, _) in &roots {
        commands.entity(e).despawn();
    }
    let map_id = link.0.session().map_id;
    let map = sim::maps::ALL
        .iter()
        .find(|d| d.id == map_id)
        .map_or("THE MAP", |d| d.name);
    let mode = front.map_or("", |f| crate::front::mode_name(f.mode));
    commands
        .spawn((LoadingRoot { gen: gen.0 }, ui::screen(ui::BG)))
        .insert(GlobalZIndex(Z))
        .with_children(|p| {
            p.spawn(ui::wordmark_at(&mode.to_uppercase(), AlignItems::Center));
            p.spawn((
                ui::strong(format!("LOADING {}", map.to_uppercase()), 18.0, ui::TEXT_COLOR),
                Node {
                    margin: UiRect::top(Val::Px(22.0)),
                    ..default()
                },
            ));
            p.spawn((Stage, ui::strong(Progress::default().stage(), 12.0, ui::FAINT)));
            p.spawn((
                Node {
                    width: Val::Px(BAR_W),
                    height: Val::Px(BAR_H),
                    margin: UiRect::vertical(Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(ui::ROW_IDLE),
                BorderColor::all(ui::RULE),
            ))
            .with_children(|track| {
                track.spawn((
                    Bar,
                    Node {
                        width: Val::Percent(0.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(ui::MARK),
                ));
                track.spawn((
                    Sweep,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(SWEEP_W),
                        height: Val::Percent(100.0),
                        left: Val::Percent(-SWEEP_W),
                        ..default()
                    },
                    BackgroundColor(ui::MARK.with_alpha(0.55)),
                ));
            });
            p.spawn(ui::label("The match has started · Esc opens the menu", 12.0, ui::FAINT));
        });
}

/// The bar and its line; lifted once the level is there, or the guard is up.
#[allow(clippy::type_complexity)]
fn update(
    mut commands: Commands,
    settle: Res<Settle>,
    time: Res<Time<Real>>,
    roots: Query<(Entity, &LoadingRoot), Without<Fading>>,
    mut bar: Query<&mut Node, (With<Bar>, Without<Sweep>)>,
    mut sweep: Query<&mut Node, (With<Sweep>, Without<Bar>)>,
    mut stage: Query<&mut Text, With<Stage>>,
) {
    let now = time.elapsed_secs_f64();
    let p = settle.now;
    for (e, root) in &roots {
        let ours = settle.gen() == Some(root.gen);
        let guard = settle.age(now) >= LOADING_GUARD_S;
        if ours && (settle.ready() || guard) {
            if !settle.ready() {
                warn!("oneshot: the loading overlay lifted at its guard, at {}", p.stage());
            }
            commands.entity(e).insert(Fading { t: 0.0, k: 1.0 });
        }
    }
    for mut n in &mut bar {
        let w = Val::Percent(p.fraction() * 100.0);
        if n.width != w {
            n.width = w;
        }
    }
    for mut n in &mut sweep {
        let busy = p.built() && p.pipelines > 0;
        n.display = if busy { Display::Flex } else { Display::None };
        if busy {
            let t = (time.elapsed_secs() / SWEEP_S).fract();
            n.left = Val::Percent(-SWEEP_W + t * (100.0 + SWEEP_W));
        }
    }
    for mut t in &mut stage {
        let line = p.stage();
        if t.0 != line {
            t.0 = line;
        }
    }
}

/// Fades whatever is fading. Bevy's UI has no group opacity, so every
/// colour under the root is scaled by the same factor (Gates' `fade`).
#[allow(clippy::type_complexity)]
fn fade(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut roots: Query<(Entity, &mut Fading)>,
    children: Query<&Children>,
    mut colours: Query<(
        Option<&mut BackgroundColor>,
        Option<&mut BorderColor>,
        Option<&mut TextColor>,
    )>,
) {
    for (root, mut f) in &mut roots {
        f.t += time.delta_secs();
        let k = (1.0 - f.t / FADE_S).max(0.0);
        if k <= 0.0 {
            commands.entity(root).despawn();
            continue;
        }
        let r = k / f.k;
        f.k = k;
        let scale = |c: &mut Color| {
            let a = c.alpha();
            c.set_alpha(a * r);
        };
        for e in std::iter::once(root).chain(children.iter_descendants(root)) {
            let Ok((bg, border, text)) = colours.get_mut(e) else {
                continue;
            };
            if let Some(mut c) = bg {
                scale(&mut c.0);
            }
            if let Some(mut c) = border {
                scale(&mut c.top);
                scale(&mut c.right);
                scale(&mut c.bottom);
                scale(&mut c.left);
            }
            if let Some(mut c) = text {
                scale(&mut c.0);
            }
        }
    }
}

/// Ready, and nothing over it: no curtain still up and no overlay.
pub(crate) fn shown(
    mut settle: ResMut<Settle>,
    curtains: Query<&BackgroundColor, With<crate::front::Curtain>>,
    overlays: Query<(), With<LoadingRoot>>,
) {
    let clear = curtains.iter().all(|c| c.0.alpha() <= 0.0) && overlays.is_empty();
    let want = settle.ready && clear;
    if settle.shown != want {
        settle.shown = want;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(built: bool, parts: (usize, usize), models: bool, warming: usize, pipelines: u32) -> Progress {
        Progress {
            built,
            parts,
            models,
            warming,
            pipelines,
        }
    }

    #[test]
    fn done_needs_every_count_and_the_bar_says_how_far() {
        assert_eq!(p(false, (0, 0), false, 0, 0).fraction(), 0.0);
        assert!(!p(true, (3, 4), true, 0, 0).done());
        assert!(!p(true, (4, 4), true, 2, 0).done());
        assert!(!p(true, (4, 4), true, 0, 5).done());
        assert!(p(true, (4, 4), true, 0, 5).built());
        assert!(p(true, (4, 4), true, 0, 0).done());
        assert_eq!(p(true, (4, 4), true, 0, 0).fraction(), 1.0);
        // A full bar is a level that's there, and not before.
        assert!(p(true, (4, 4), true, 0, 5).fraction() < 1.0);
        assert!(p(true, (2, 4), false, 0, 0).fraction() > p(true, (0, 4), false, 0, 0).fraction());
        // A level with no parts to dress (none yet counted) isn't dressed
        // before it's built.
        assert_eq!(p(false, (0, 0), true, 0, 0).fraction(), 0.25);
    }

    #[test]
    fn the_line_names_what_is_left() {
        assert_eq!(p(false, (0, 0), false, 0, 0).stage(), "BUILDING THE LEVEL");
        assert_eq!(p(true, (1, 3), false, 0, 0).stage(), "DRESSING THE LEVEL · 1 OF 3");
        assert_eq!(p(true, (3, 3), true, 1, 0).stage(), "WARMING THE MODELS");
        assert_eq!(p(true, (3, 3), true, 0, 7).stage(), "COMPILING SHADERS · 7");
        assert_eq!(p(true, (3, 3), true, 0, 0).stage(), "READY");
    }
}
