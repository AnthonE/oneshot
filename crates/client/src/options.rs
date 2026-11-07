//! The settings screen, after Gates' `client/src/render/settings.rs` (MIT):
//! a category rail down the left with the picked one blocked out in the
//! blue, a pane of rows on the right, each a label and its control, and a
//! foot with BACK, RESET TAB and where it's all kept. A number is a `-`
//! value `+` stepper, a toggle a button that says ON or OFF; a key cell
//! captures the next key, mouse button or wheel turn (Esc cancels, a right
//! click on a cell not waiting clears it), and a key taken from another
//! action says where from. Everything saves as it
//! changes (`settings.rs`) and applies at once: over a match the match shows
//! through, so a wider view or a lower rung is seen as it's chosen. BACK or
//! Esc: the Esc menu, or the title.
//!
//! GRAPHICS is the ladder (`look/quality.rs`): QUALITY writes every row
//! under it and reads CUSTOM once one moves. A row this target can't move
//! is drawn as a fact saying why (WebGL2's walls, a browser's own frame
//! pacing), never as a control that does nothing; what this GPU refused of
//! the rest is the "on this machine" line, derived from what is drawn.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::binds::{normalize, Action, Bind, Binds, MOUSE, SPECS};
use crate::look::quality::{self, Tier};
use crate::look::rig;
use crate::settings::{Disk, Knob, Settings};
use crate::{ui, Screen};

#[derive(Component)]
struct OptionsRoot;

/// The part that scrolls.
#[derive(Component)]
struct Pane;

#[derive(Component, Clone, Copy)]
struct Adjust {
    knob: Knob,
    delta: i32,
}

#[derive(Component, Clone, Copy)]
struct BindCell {
    action: Action,
    slot: usize,
}

#[derive(Component, Clone, Copy)]
struct Category(usize);

#[derive(Component)]
struct Back;

#[derive(Component)]
struct ResetTab;

/// The rail.
pub const CATEGORIES: [&str; 4] = ["MOUSE AND VIEW", "SOUND", "GRAPHICS", "KEYS"];
/// The rail's GRAPHICS.
pub const GRAPHICS: usize = 2;
/// The rail's KEYS (the title's CONTROLS opens on it).
pub const KEYS: usize = 3;

/// The screen's own state: the picked category, the cell waiting for a
/// key, the line under the pane, and where the pane was scrolled to (kept
/// across a rebuild).
#[derive(Resource, Default)]
pub struct Page {
    pub cat: usize,
    capturing: Option<(Action, usize)>,
    note: String,
    scroll: f32,
    dirty: bool,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Page>()
        .add_systems(
            OnEnter(Screen::Settings),
            (crate::pause::free_cursor, enter, build).chain(),
        )
        .add_systems(OnExit(Screen::Settings), ui::despawn_all::<OptionsRoot>)
        .add_systems(
            Update,
            (capture, click, keys, scroll, rebuild)
                .chain()
                .run_if(in_state(Screen::Settings)),
        );
}

fn enter(mut st: ResMut<Page>) {
    st.capturing = None;
    st.note.clear();
    st.scroll = 0.0;
    st.dirty = false;
}

/// Where BACK goes: the Esc menu in a match (or a picture), else the title.
fn back(in_match: Option<&crate::front::InMatch>) -> Screen {
    match in_match {
        Some(m) if !m.0 => Screen::Title,
        _ => Screen::Paused,
    }
}

fn build(
    mut commands: Commands,
    settings: Res<Settings>,
    binds: Res<Binds>,
    st: Res<Page>,
    disk: Option<Res<Disk>>,
    in_match: Option<Res<crate::front::InMatch>>,
    tier: Res<Tier>,
) {
    let to = back(in_match.as_deref());
    spawn(&mut commands, &settings, &binds, &st, disk.as_deref(), to, &tier);
}

/// The frame every row shares: label left, control right.
fn frame() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(20.0),
        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
        flex_shrink: 0.0,
        ..default()
    }
}

fn spawn(
    commands: &mut Commands,
    settings: &Settings,
    binds: &Binds,
    st: &Page,
    disk: Option<&Disk>,
    to: Screen,
    tier: &Tier,
) {
    let over_match = to == Screen::Paused;
    let (root_bg, panel_bg) = if over_match {
        (Color::srgba(0.055, 0.051, 0.043, 0.55), ui::PANE_BG)
    } else {
        (Color::NONE, ui::PANEL_BG)
    };
    let saved = disk.map_or("settings are not being saved this run".into(), Disk::describe);
    let back_to = if over_match { "the match" } else { "the title" };
    commands
        .spawn((OptionsRoot, ui::shell(), BackgroundColor(root_bg), GlobalZIndex(10)))
        .with_children(|root| {
            if !over_match {
                root.spawn(ui::scrim());
            }
            root.spawn((
                ui::strong("SETTINGS", 30.0, ui::TITLE),
                ui::TEXT_SHADOW,
                Node {
                    margin: UiRect::new(Val::Px(34.0), Val::Px(0.0), Val::Px(26.0), Val::Px(16.0)),
                    ..default()
                },
            ));
            // The rail and the pane, side by side, both full height.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Px(14.0),
                padding: UiRect::new(Val::Px(34.0), Val::Px(34.0), Val::Px(0.0), Val::Px(12.0)),
                ..default()
            })
            .with_children(|body| {
                body.spawn((
                    Node {
                        width: Val::Px(250.0),
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(panel_bg),
                    BorderColor::all(ui::RULE),
                ))
                .with_children(|rail| {
                    for (i, name) in CATEGORIES.iter().enumerate() {
                        let on = i == st.cat;
                        let (idle, over) = if on {
                            (ui::ACCENT, ui::ACCENT_HOVER)
                        } else {
                            (ui::ROW_IDLE, ui::ROW_HOVER)
                        };
                        rail.spawn((
                            Button,
                            Category(i),
                            ui::Hover::new(idle, over),
                            Node {
                                width: Val::Percent(100.0),
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(11.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(idle),
                            BorderColor::all(if on { ui::ACCENT } else { ui::RULE }),
                            children![ui::strong(*name, 16.0, if on { ui::TEXT_COLOR } else { ui::DIM })],
                        ));
                    }
                });
                body.spawn((
                    Pane,
                    Node {
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(3.0),
                        padding: UiRect::all(Val::Px(14.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(panel_bg),
                    BorderColor::all(ui::RULE),
                    ScrollPosition(Vec2::new(0.0, st.scroll)),
                ))
                .with_children(|pane| match st.cat {
                    0 => {
                        number(pane, settings, Knob::Sensitivity, "MOUSE SENSITIVITY", "CS2 / Apex");
                        toggle(pane, settings, Knob::Invert, "INVERT LOOK");
                        number(pane, settings, Knob::Fov, "FIELD OF VIEW", "across");
                        note(
                            pane,
                            "Sensitivity is on CS2's and Apex's scale (0.022° a count). Scoped, the mouse turns slower by the zoom, so the world moves under the crosshair at one speed. The field of view is across a 16:9 screen.",
                        );
                    }
                    1 => {
                        number(pane, settings, Knob::Master, "VOLUME", "everything");
                        number(pane, settings, Knob::Effects, "GAME SOUNDS", "shots, steps, hits");
                        // The party's voice is the page's (`web/voice.js`).
                        if cfg!(target_arch = "wasm32") {
                            number(pane, settings, Knob::Voice, "VOICE", "your party");
                        }
                    }
                    GRAPHICS => graphics(pane, settings, tier),
                    _ => {
                        // TALK is the page's voice's (`web/voice.js`).
                        for s in SPECS
                            .iter()
                            .filter(|s| s.action != Action::Talk || cfg!(target_arch = "wasm32"))
                        {
                            key_row(pane, binds, st, s.action, s.name);
                        }
                        note(
                            pane,
                            "Halo 2's keys by default. Every control binds, the mouse's buttons and its wheel too. Zoom and the left gun's trigger may share a button: with a gun in each hand there is no scope.",
                        );
                    }
                });
            });
            // The foot.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(18.0),
                margin: UiRect::new(Val::Px(34.0), Val::Px(34.0), Val::Px(0.0), Val::Px(16.0)),
                ..default()
            })
            .with_children(|foot| {
                foot.spawn((ui::button(120.0, true), Back))
                    .with_child(ui::strong("BACK", 14.0, ui::TITLE));
                foot.spawn((ui::button(150.0, false), ResetTab))
                    .with_child(ui::strong("RESET TAB", 14.0, ui::TEXT_COLOR));
                foot.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    ..default()
                })
                .with_children(|col| {
                    let hint = if st.capturing.is_some() {
                        "Press the key or mouse button, or turn the wheel · Esc cancels".to_string()
                    } else if !st.note.is_empty() {
                        st.note.clone()
                    } else if st.cat == KEYS {
                        "Click a key to change it · right click clears it".to_string()
                    } else if st.cat == GRAPHICS && settings.held.is_some() {
                        let by = if cfg!(target_arch = "wasm32") {
                            "?lite=1, ?quality="
                        } else {
                            "--lite, --quality"
                        };
                        format!("This run draws another rung ({by}): yours is kept until you change a row")
                    } else {
                        String::new()
                    };
                    if !hint.is_empty() {
                        let c = if st.note.is_empty() { ui::DIM } else { ui::TEXT_COLOR };
                        col.spawn(ui::label(hint, 13.0, c));
                    }
                    col.spawn(ui::label(
                        format!("BACK or Esc returns to {back_to} · {saved}"),
                        12.0,
                        ui::FAINT,
                    ));
                });
            });
        });
}

fn note(p: &mut ChildSpawnerCommands, text: &str) {
    p.spawn((
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(10.0)),
            max_width: Val::Px(760.0),
            flex_shrink: 0.0,
            ..default()
        },
        children![ui::label(text, 14.0, ui::DIM)],
    ));
}

/// A row with nothing to turn: a label and what is so.
fn fact(p: &mut ChildSpawnerCommands, name: &str, value: impl Into<String>) {
    p.spawn((frame(), BackgroundColor(ui::ROW_IDLE))).with_children(|r| {
        r.spawn(ui::strong(name, 15.0, ui::DIM));
        r.spawn((
            Node {
                max_width: Val::Px(520.0),
                justify_content: JustifyContent::FlexEnd,
                ..default()
            },
            children![ui::label(value, 13.0, ui::DIM)],
        ));
    });
}

/// SETTINGS > GRAPHICS: the preset, its rows, what this machine does with
/// them, and the window.
fn graphics(pane: &mut ChildSpawnerCommands, s: &Settings, tier: &Tier) {
    let webgl2 = crate::look::gpu::WEBGL2;
    let browser = cfg!(target_arch = "wasm32");
    number(pane, s, Knob::Quality, "QUALITY", "a preset for the rows below");
    pane.spawn(ui::group("SHADE"));
    let floor = tier.shadow_m > tier.gfx.shadow_m;
    number(
        pane,
        s,
        Knob::ShadowDistance,
        "SHADOW DISTANCE",
        if floor {
            "at least across this map"
        } else {
            "how far the sun casts"
        },
    );
    if webgl2 {
        fact(pane, "CASCADES", "1: WebGL2 draws one, over the whole distance");
    } else {
        number(pane, s, Knob::Cascades, "CASCADES", "slices of the distance");
    }
    number(pane, s, Knob::ShadowMap, "SHADOW MAP", "texels a side");
    fact(pane, "  at the far edge", far_edge(s, tier));
    pane.spawn(ui::group("IMAGE"));
    if webgl2 {
        fact(pane, "AMBIENT OCCLUSION", "not in WebGL2: it has no compute");
    } else {
        number(pane, s, Knob::Ambient, "AMBIENT OCCLUSION", "contact shading");
    }
    number(
        pane,
        s,
        Knob::Aa,
        "ANTI-ALIASING",
        if webgl2 {
            "FXAA or SMAA; no TAA in WebGL2"
        } else {
            "TAA is the smoothest"
        },
    );
    if webgl2 {
        fact(pane, "BLOOM", "not in WebGL2: the glows are drawn to read without it");
    } else {
        toggle(pane, s, Knob::Bloom, "BLOOM");
    }
    number(
        pane,
        s,
        Knob::RenderScale,
        "RENDER SCALE",
        "the world's resolution; the HUD stays sharp",
    );
    fact(
        pane,
        "SKY",
        if tier.atmosphere {
            "the atmosphere: this GPU draws the sky and the air"
        } else if webgl2 {
            "drawn per pixel, and the same air as a haze: WebGL2 has no compute"
        } else {
            "drawn per pixel, and the same air as a haze: this GPU can't filter the atmosphere's tables"
        },
    );
    number(pane, s, Knob::Clouds, "CLOUDS", "flat, or a raymarched slab");
    fact(
        pane,
        "  light shafts",
        if tier.shafts() {
            "through a misty map's air, at HIGH"
        } else if tier.atmosphere {
            "at HIGH clouds: the sun's beams through a misty map's air"
        } else if tier.gfx.clouds == crate::look::quality::Clouds::High {
            "through a misty map's air, at HIGH: marched over the walls and the sky, with no volumetric pass here"
        } else {
            "at HIGH clouds, over the walls and the sky: no volumetric pass here, so a march of the sun's shadow"
        },
    );
    pane.spawn(ui::group("WORLD"));
    fact(
        pane,
        "GRASS",
        "the same on every rung: it can cover a crouching fighter",
    );
    number(pane, s, Knob::Fx, "EFFECTS", "sparks, tracers, flash lights");
    pane.spawn(ui::group("ON THIS MACHINE"));
    note(pane, &target_note(s, tier));
    pane.spawn(ui::group("WINDOW"));
    toggle(pane, s, Knob::Fullscreen, "FULLSCREEN");
    if browser {
        fact(pane, "VSYNC", "always: the browser draws once a refresh");
        fact(pane, "FRAME CAP", "the display's: the browser paces every frame");
    } else {
        toggle(pane, s, Knob::Vsync, "VSYNC");
        number(pane, s, Knob::FrameCap, "FRAME CAP", "frames a second");
    }
    note(
        pane,
        "No row shows anyone another hides: the shade reaches across the map on every rung, smoke, marks, grass \
         and stones are the same for all.",
    );
}

/// The shade at the far edge of the last cascade, as drawn: centimetres of
/// world per texel, and what the maps cost.
fn far_edge(s: &Settings, tier: &Tier) -> String {
    let g = &tier.gfx;
    format!(
        "{:.0} cm a texel at {:.0} m · {:.0} MiB",
        quality::far_texel_m(g.cascades, tier.shadow_m, g.shadow_map_px, rig::fov_tall(s.fov)) * 100.0,
        tier.shadow_m,
        quality::shadow_vram_mb(g.cascades, g.shadow_map_px),
    )
}

/// What this machine draws differently from the rows, in a sentence:
/// derived by diffing them against what is drawn, so a clamp shows up here
/// the day it's added and stops being claimed the day it goes.
fn target_note(s: &Settings, tier: &Tier) -> String {
    let (want, got) = (s.gfx, tier.gfx);
    let mut notes: Vec<String> = Vec::new();
    if tier.shadow_m > want.shadow_m {
        notes.push(format!(
            "shadows reach {:.0} m, across this map, for everyone",
            tier.shadow_m
        ));
    }
    if got.ao != want.ao {
        notes.push("no ambient occlusion: this GPU has no room for its prepasses".into());
    }
    if got.aa != want.aa {
        notes.push(format!(
            "{} for {}: no room for TAA's history",
            got.aa.name().to_uppercase(),
            want.aa.name().to_uppercase()
        ));
    }
    if got.bloom != want.bloom {
        notes.push("no bloom: this GPU can't draw its mip chain".into());
    }
    if notes.is_empty() {
        "Everything above is what is drawn.".into()
    } else {
        let mut line = notes.join("; ");
        line.push('.');
        let mut c = line.chars();
        c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
    }
}

fn number(p: &mut ChildSpawnerCommands, s: &Settings, knob: Knob, name: &str, unit: &str) {
    p.spawn((frame(), BackgroundColor(ui::ROW_IDLE))).with_children(|r| {
        r.spawn(ui::strong(name, 15.0, ui::TEXT_COLOR));
        r.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(ui::label(unit, 12.0, ui::FAINT));
            c.spawn((ui::stepper(), Adjust { knob, delta: -1 }))
                .with_child(ui::strong("-", 16.0, ui::TEXT_COLOR));
            c.spawn((
                Node {
                    width: Val::Px(84.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![ui::strong(s.value(knob), 15.0, ui::TEXT_COLOR)],
            ));
            c.spawn((ui::stepper(), Adjust { knob, delta: 1 }))
                .with_child(ui::strong("+", 16.0, ui::TEXT_COLOR));
        });
    });
}

fn toggle(p: &mut ChildSpawnerCommands, s: &Settings, knob: Knob, name: &str) {
    p.spawn((frame(), BackgroundColor(ui::ROW_IDLE))).with_children(|r| {
        r.spawn(ui::strong(name, 15.0, ui::TEXT_COLOR));
        r.spawn((
            Button,
            Adjust { knob, delta: 0 },
            ui::Hover::default(),
            BackgroundColor(ui::ROW_IDLE),
            BorderColor::all(ui::RULE),
            Node {
                width: Val::Px(92.0),
                height: Val::Px(26.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            children![ui::strong(s.value(knob), 14.0, ui::TEXT_COLOR)],
        ));
    });
}

fn key_row(p: &mut ChildSpawnerCommands, binds: &Binds, st: &Page, action: Action, name: &str) {
    p.spawn((frame(), BackgroundColor(ui::ROW_IDLE))).with_children(|r| {
        r.spawn(ui::strong(name.to_uppercase(), 15.0, ui::TEXT_COLOR));
        r.spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|c| {
            for (slot, b) in binds.slots(action).into_iter().enumerate() {
                let waiting = st.capturing == Some((action, slot));
                let (text, colour) = match (waiting, b) {
                    (true, _) => ("PRESS A KEY", ui::TITLE),
                    (false, Some(b)) => (b.label(), ui::TEXT_COLOR),
                    (false, None) => ("-", ui::FAINT),
                };
                let (idle, over) = if waiting {
                    (ui::ACCENT, ui::ACCENT_HOVER)
                } else {
                    (ui::ROW_IDLE, ui::ROW_HOVER)
                };
                c.spawn((
                    Button,
                    BindCell { action, slot },
                    ui::Hover::new(idle, over),
                    Node {
                        width: Val::Px(132.0),
                        height: Val::Px(26.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(idle),
                    BorderColor::all(if waiting { ui::ACCENT } else { ui::RULE }),
                    children![ui::strong(text, 14.0, colour)],
                ));
            }
        });
    });
}

/// A key cell waiting: the next key or side button pressed is its bind
/// (taken from wherever it was); Esc cancels; a key that can't be bound
/// says so. Runs first, so the key it takes does nothing else this frame.
fn capture(
    mut st: ResMut<Page>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut binds: ResMut<Binds>,
) {
    let Some((action, slot)) = st.capturing else {
        return;
    };
    if keys.just_pressed(KeyCode::Escape) {
        keys.clear_just_pressed(KeyCode::Escape);
        st.capturing = None;
        st.note = "Kept as it was".into();
        st.dirty = true;
        return;
    }
    let pressed = keys
        .get_just_pressed()
        .next()
        .map(|k| Bind::Key(normalize(*k)))
        .or_else(|| {
            MOUSE
                .iter()
                .find(|(b, ..)| mouse.just_pressed(*b))
                .map(|(b, ..)| Bind::Mouse(*b))
        })
        .or_else(|| (wheel.delta.y != 0.0).then_some(Bind::Wheel));
    let Some(bind) = pressed else {
        return;
    };
    // Taken, so `click` doesn't read the same press as a click on a cell
    // (a left click there would wait again, a right click would clear it).
    match bind {
        Bind::Key(k) => {
            keys.clear_just_pressed(k);
        }
        Bind::Mouse(b) => {
            mouse.clear_just_pressed(b);
        }
        Bind::Wheel => {}
    }
    st.capturing = None;
    st.dirty = true;
    if !bind.bindable() {
        st.note = format!("{} can't be bound", bind.label());
        return;
    }
    st.note = match binds.set(action, slot, Some(bind)) {
        Some(from) => {
            let gone = if binds.slots(from).iter().all(Option::is_none) {
                format!("; {} has no key now", from.spec().name)
            } else {
                String::new()
            };
            format!("{} moved here from {}{gone}", bind.label(), from.spec().name)
        }
        None => format!("{} is {}", action.spec().name, bind.label()),
    };
}

/// Clicks: the rail picks a category, a stepper or toggle turns its knob,
/// a key cell waits for a key, a right click on one clears it, RESET TAB
/// puts the category's defaults back, BACK goes back.
#[allow(clippy::too_many_arguments)]
fn click(
    cats: Query<(&Interaction, &Category), Changed<Interaction>>,
    adjust: Query<(&Interaction, &Adjust), Changed<Interaction>>,
    cells: Query<(Entity, &Interaction, &BindCell)>,
    pad: Res<crate::pad::PadPressed>,
    backs: Query<&Interaction, (Changed<Interaction>, With<Back>)>,
    resets: Query<&Interaction, (Changed<Interaction>, With<ResetTab>)>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut settings: ResMut<Settings>,
    mut binds: ResMut<Binds>,
    mut st: ResMut<Page>,
    mut next: ResMut<NextState<Screen>>,
    in_match: Option<Res<crate::front::InMatch>>,
) {
    if st.capturing.is_none() {
        for (i, c) in &cats {
            if *i == Interaction::Pressed && c.0 != st.cat {
                st.cat = c.0;
                st.scroll = 0.0;
                st.note.clear();
                st.dirty = true;
            }
        }
        if resets.iter().any(|i| *i == Interaction::Pressed) {
            let d = Settings::default();
            match st.cat {
                0 => {
                    settings.sensitivity = d.sensitivity;
                    settings.invert = d.invert;
                    settings.fov = d.fov;
                }
                1 => {
                    settings.master = d.master;
                    settings.effects = d.effects;
                    settings.voice = d.voice;
                }
                GRAPHICS => settings.reset_graphics(),
                _ => *binds = Binds::default(),
            }
            st.note = format!("{} is back to its defaults", CATEGORIES[st.cat]);
            st.dirty = true;
        }
    }
    for (i, a) in &adjust {
        if *i == Interaction::Pressed {
            settings.adjust(a.knob, a.delta);
            st.note.clear();
            st.dirty = true;
        }
    }
    for (e, i, c) in &cells {
        // A left click or the pad's A: the right button clears, and a held
        // button is no new press.
        let pressed = mouse.just_pressed(MouseButton::Left) || pad.0 == Some(e);
        if *i == Interaction::Pressed && pressed && st.capturing.is_none() {
            st.capturing = Some((c.action, c.slot));
            st.note.clear();
            st.dirty = true;
        } else if *i == Interaction::Hovered && mouse.just_pressed(MouseButton::Right) && st.capturing.is_none() {
            binds.set(c.action, c.slot, None);
            st.note = format!("{}: key {} cleared", c.action.spec().name, c.slot + 1);
            st.dirty = true;
        }
    }
    if backs.iter().any(|i| *i == Interaction::Pressed) && st.capturing.is_none() {
        next.set(back(in_match.as_deref()));
    }
}

/// Esc goes back (when no cell is waiting: then it cancels, `capture`).
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    st: Res<Page>,
    mut next: ResMut<NextState<Screen>>,
    in_match: Option<Res<crate::front::InMatch>>,
) {
    if st.capturing.is_none() && keys.just_pressed(KeyCode::Escape) {
        next.set(back(in_match.as_deref()));
    }
}

/// The wheel scrolls the list; where it is scrolled to (by the wheel, or
/// the pad bringing its focus into view: `pad.rs`) is kept for a rebuild.
fn scroll(
    mut wheel: MessageReader<MouseWheel>,
    mut pane: Query<&mut ScrollPosition, With<Pane>>,
    mut st: ResMut<Page>,
) {
    let dy: f32 = wheel
        .read()
        .map(|w| match w.unit {
            MouseScrollUnit::Line => w.y * 32.0,
            MouseScrollUnit::Pixel => w.y,
        })
        .sum();
    // A pane about to be built again (another category) starts from the
    // scroll that click set, not the old pane's.
    if st.dirty {
        return;
    }
    for mut p in &mut pane {
        if dy != 0.0 {
            p.y = (p.y - dy).max(0.0);
        }
        if p.y != st.scroll {
            st.scroll = p.y;
        }
    }
}

/// Built again when anything on it changed (Gates' way: simpler than
/// patching, and a screen this size builds in no time).
#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    mut st: ResMut<Page>,
    roots: Query<Entity, With<OptionsRoot>>,
    settings: Res<Settings>,
    binds: Res<Binds>,
    disk: Option<Res<Disk>>,
    in_match: Option<Res<crate::front::InMatch>>,
    tier: Res<Tier>,
) {
    // Also when something else moved them: F11, the page leaving
    // fullscreen, the frame-rate watch, the map's shade.
    if !st.dirty && !settings.is_changed() && !tier.is_changed() {
        return;
    }
    st.dirty = false;
    for e in &roots {
        commands.entity(e).despawn();
    }
    spawn(
        &mut commands,
        &settings,
        &binds,
        &st,
        disk.as_deref(),
        back(in_match.as_deref()),
        &tier,
    );
}
