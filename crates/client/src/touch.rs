//! Touch controls, for a phone or a tablet (and a desktop's touchscreen):
//! a stick wherever the left thumb lands, the right thumb's drag turns the
//! view, Halo's buttons in an arc round FIRE, and MENU under the shield.
//! The grenades in the HUD's corner are a button too (a tap switches
//! them), and so are the clock and the scores, which move to the bottom's
//! middle, between the thumbs (`hud::score_corner`): held, the board.
//!
//! The first touch turns them on, a key or a mouse button off; `touch` in
//! the page's options (`?touch=1`, or a device whose pointer is a finger:
//! `web/boot.js`) has them on from the start.
//!
//! Like a key, it decides nothing: a button holds its actions for
//! `binds::Controls`, the stick is `pump`'s `mx` and `mz`, the drag its
//! mouse. Laid out in the window's logical px (a CSS px in a browser, about
//! one size under every thumb), drawn under the UI's scale. Off a match a
//! finger's drag is the wheel, so the title's and the settings' lists
//! scroll under it.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::window::PrimaryWindow;

use crate::binds::{Action, Binds};
use crate::Screen;

/// The buttons, in the order they are drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pad {
    Fire,
    Jump,
    Crouch,
    Reload,
    Zoom,
    Grenade,
    Melee,
    Use,
    Swap,
    Menu,
    /// The HUD's grenades, tapped: the other kind.
    GrenSwitch,
    /// The HUD's clock and scores, held: the board.
    Scores,
}

pub const PADS: [Pad; 12] = [
    Pad::Fire,
    Pad::Jump,
    Pad::Crouch,
    Pad::Reload,
    Pad::Zoom,
    Pad::Grenade,
    Pad::Melee,
    Pad::Use,
    Pad::Swap,
    Pad::Menu,
    Pad::GrenSwitch,
    Pad::Scores,
];

const fn bit(a: Action) -> u32 {
    1 << a as u32
}

impl Pad {
    /// What it holds down, as a key bound to them would.
    pub const fn actions(self) -> u32 {
        match self {
            Pad::Fire => bit(Action::Fire),
            Pad::Jump => bit(Action::Jump),
            // At a one-seater's controls crouch is its boost (`pump`).
            Pad::Crouch => bit(Action::Crouch) | bit(Action::Boost),
            Pad::Reload => bit(Action::Reload),
            // Mouse 2's two: with a gun in each hand, the left one's
            // trigger; in a flier, its bomb.
            Pad::Zoom => bit(Action::Zoom) | bit(Action::FireLeft),
            Pad::Grenade => bit(Action::Grenade),
            Pad::Melee => bit(Action::Melee),
            Pad::Use => bit(Action::Use),
            Pad::Swap => bit(Action::Swap),
            Pad::Menu => 0,
            Pad::GrenSwitch => bit(Action::GrenSwitch),
            Pad::Scores => bit(Action::Scores),
        }
    }

    /// Its face's word, and what a prompt calls it (`key`).
    pub const fn label(self) -> &'static str {
        match self {
            Pad::Fire => "FIRE",
            Pad::Jump => "JUMP",
            Pad::Crouch => "CROUCH",
            Pad::Reload => "RELOAD",
            Pad::Zoom => "ZOOM",
            Pad::Grenade => "GRENADE",
            Pad::Melee => "MELEE",
            Pad::Use => "USE",
            Pad::Swap => "SWAP",
            Pad::Menu => "MENU",
            Pad::GrenSwitch => "GRENADES",
            Pad::Scores => "SCORES",
        }
    }
}

/// What a prompt calls `a`'s control: the button's word on a touchscreen,
/// else the key's (`Binds::key`).
pub fn key(binds: &Binds, touch: &Touch, a: Action) -> &'static str {
    match PADS.iter().find(|p| p.actions() & bit(a) != 0) {
        Some(p) if touch.on => p.label(),
        _ => binds.key(a),
    }
}

/// A button's shape, logical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Round { at: Vec2, r: f32 },
    Box(Rect),
}

/// Where everything is, logical px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Layout {
    /// By `PADS`' order; a zone the HUD hasn't laid out yet is `None`.
    pub pads: [Option<Shape>; PADS.len()],
    /// The stick's home (drawn there at rest) and its reach.
    pub stick_home: Vec2,
    pub stick_r: f32,
    /// Left of this a finger off the buttons is the stick's; right, the
    /// view's.
    pub split: f32,
}

/// The phone the layout is drawn for (a landscape phone's CSS px), and how
/// far it grows or shrinks for another screen.
const PHONE: Vec2 = Vec2::new(844.0, 390.0);
const GROW_MIN: f32 = 0.8;
const GROW_MAX: f32 = 1.35;
/// Off the screen's edges (and a phone's rounded corners).
const EDGE: f32 = 14.0;
/// FIRE's centre in from the bottom right corner, and its radius.
const FIRE_IN: Vec2 = Vec2::new(118.0, 108.0);
const FIRE_R: f32 = 46.0;
/// The arc round FIRE: each button's angle (degrees, anticlockwise from
/// the right), how far out and its radius.
const ARC: [(Pad, f32, f32, f32); 8] = [
    (Pad::Jump, -40.0, 98.0, 34.0),
    (Pad::Crouch, 40.0, 92.0, 26.0),
    (Pad::Reload, 92.0, 92.0, 26.0),
    (Pad::Zoom, 136.0, 92.0, 28.0),
    (Pad::Grenade, 180.0, 92.0, 27.0),
    (Pad::Melee, 224.0, 92.0, 27.0),
    (Pad::Use, 152.0, 168.0, 26.0),
    (Pad::Swap, 112.0, 172.0, 26.0),
];
/// MENU, under the shield: its radius, and how far below it.
const MENU_R: f32 = 19.0;
const MENU_GAP: f32 = 8.0;
/// The shield's foot, in the HUD's own px (`hud.rs`: its top, the bar,
/// the gap and the health line under it).
const SHIELD_FOOT_UI: f32 = 52.0;
/// The stick at rest: in from the bottom left corner, and its reach.
const STICK_IN: Vec2 = Vec2::new(104.0, 96.0);
const STICK_R: f32 = 60.0;
/// The stick's share of the screen, from the left.
const SPLIT: f32 = 0.45;
/// A finger this far past a round button's rim (a share of its radius)
/// still presses it; a zone is grown this much, px.
const HIT: f32 = 1.2;
const ZONE_SLOP: f32 = 8.0;
/// The stick's dead middle and where it is full over, shares of its reach.
const DEAD: f32 = 0.14;
const FULL: f32 = 0.85;

/// The layout for a `w` by `h` window whose UI is drawn at `ui` (`ui.rs`),
/// with the HUD's grenades and scores where they were last laid out.
pub fn layout(w: f32, h: f32, ui: f32, gren: Option<Rect>, board: Option<Rect>) -> Layout {
    let k = (w / PHONE.x).min(h / PHONE.y).clamp(GROW_MIN, GROW_MAX);
    let m = EDGE * k;
    let fire = Vec2::new(w - m - FIRE_IN.x * k, h - m - FIRE_IN.y * k);
    let mut pads = [None; PADS.len()];
    let mut put = |p: Pad, s: Shape| pads[PADS.iter().position(|q| *q == p).unwrap_or(0)] = Some(s);
    put(
        Pad::Fire,
        Shape::Round {
            at: fire,
            r: FIRE_R * k,
        },
    );
    for (p, deg, ring, r) in ARC {
        let a = deg.to_radians();
        let at = fire + ring * k * Vec2::new(a.cos(), -a.sin());
        put(p, Shape::Round { at, r: r * k });
    }
    put(
        Pad::Menu,
        Shape::Round {
            at: Vec2::new(w * 0.5, SHIELD_FOOT_UI * ui + (MENU_GAP + MENU_R) * k),
            r: MENU_R * k,
        },
    );
    if let Some(g) = gren {
        put(Pad::GrenSwitch, Shape::Box(g));
    }
    if let Some(b) = board {
        put(Pad::Scores, Shape::Box(b));
    }
    Layout {
        pads,
        stick_home: Vec2::new(m + STICK_IN.x * k, h - m - STICK_IN.y * k),
        stick_r: STICK_R * k,
        split: w * SPLIT,
    }
}

impl Layout {
    /// What a finger coming down at `p` is: the nearest button it is on,
    /// else the stick's or the view's by its side of the screen.
    fn role(&self, p: Vec2) -> Role {
        let mut best: Option<(f32, Pad)> = None;
        for (i, s) in self.pads.iter().enumerate() {
            let near = match s {
                Some(Shape::Round { at, r }) => p.distance(*at) / (r * HIT),
                Some(Shape::Box(b)) if b.inflate(ZONE_SLOP).contains(p) => 0.5,
                _ => continue,
            };
            if near <= 1.0 && best.is_none_or(|(b, _)| near < b) {
                best = Some((near, PADS[i]));
            }
        }
        match best {
            Some((_, pad)) => Role::Pad(pad),
            None if p.x < self.split => Role::Stick { origin: p },
            None => Role::Look,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    /// Came down off a match (or the match went away under it): a drag
    /// scrolls, nothing is held.
    Menu,
    /// The stick, from where it came down (it follows a finger past its
    /// reach).
    Stick { origin: Vec2 },
    /// Turns the view.
    Look,
    /// Holds a button; FIRE's finger turns the view too.
    Pad(Pad),
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: u64,
    role: Role,
    at: Vec2,
}

/// The fingers, and what they hold this frame.
#[derive(Resource, Default)]
pub struct Touch {
    /// Touch controls are on: a finger was the last thing played with.
    pub on: bool,
    /// On, in a match, and it is on screen: what a finger coming down plays.
    pub live: bool,
    pub lay: Layout,
    fingers: Vec<Finger>,
    /// The stick, each way -1 to 1, forward up.
    pub stick: Vec2,
    /// The view's drag this frame, logical px.
    pub turn: Vec2,
    /// A menu's drag this frame, logical px down.
    pub scroll: f32,
    /// A finger came down on the view this frame (watching: the next one).
    pub tapped: bool,
    /// MENU came down this frame.
    pub menu: bool,
    held: u32,
    just: u32,
}

impl Touch {
    /// Actions held now, by `Action` bit: a key's `pressed`.
    pub fn held(&self) -> u32 {
        if self.live {
            self.held
        } else {
            0
        }
    }

    /// Actions pressed this frame: a key's `just_pressed`, which a tap that
    /// came and went inside one frame still is.
    pub fn just(&self) -> u32 {
        if self.live {
            self.just
        } else {
            0
        }
    }

    pub fn is_down(&self, p: Pad) -> bool {
        self.live && self.fingers.iter().any(|f| f.role == Role::Pad(p))
    }

    /// Where the stick is drawn (its base, and the finger on it), if a
    /// finger holds it.
    pub fn stick_at(&self) -> Option<(Vec2, Vec2)> {
        self.fingers.iter().find_map(|f| match f.role {
            Role::Stick { origin } if self.live => Some((origin, f.at)),
            _ => None,
        })
    }

    /// A frame's fingers start here.
    pub fn begin(&mut self, live: bool) {
        self.just = 0;
        self.turn = Vec2::ZERO;
        self.scroll = 0.0;
        self.tapped = false;
        self.menu = false;
        if self.live && !live {
            // Off the match (the menu opened, the match ended): what was
            // held is let go, and comes back only from a fresh press.
            for f in &mut self.fingers {
                f.role = Role::Menu;
            }
        }
        self.live = live;
    }

    pub fn feed(&mut self, id: u64, phase: TouchPhase, at: Vec2) {
        match phase {
            TouchPhase::Started => {
                self.on = true;
                self.fingers.retain(|f| f.id != id);
                let role = if self.live { self.lay.role(at) } else { Role::Menu };
                match role {
                    Role::Pad(Pad::Menu) => self.menu = true,
                    Role::Pad(p) => self.just |= p.actions(),
                    Role::Look => self.tapped = true,
                    _ => {}
                }
                self.fingers.push(Finger { id, role, at });
            }
            TouchPhase::Moved => {
                let r = self.lay.stick_r;
                let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) else {
                    return;
                };
                let d = at - f.at;
                f.at = at;
                match &mut f.role {
                    Role::Look | Role::Pad(Pad::Fire) => self.turn += d,
                    Role::Stick { origin } => {
                        let off = at - *origin;
                        if off.length() > r {
                            *origin = at - off.normalize() * r;
                        }
                    }
                    Role::Menu => self.scroll += d.y,
                    Role::Pad(_) => {}
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => self.fingers.retain(|f| f.id != id),
        }
    }

    /// The frame's fingers are in: what they hold now.
    pub fn end(&mut self) {
        self.held = self
            .fingers
            .iter()
            .filter_map(|f| match f.role {
                Role::Pad(p) => Some(p.actions()),
                _ => None,
            })
            .fold(0, |a, b| a | b);
        self.stick = self
            .stick_at()
            .map_or(Vec2::ZERO, |(o, at)| stick_of(at - o, self.lay.stick_r));
    }

    /// A key or a mouse button: the touch controls go away.
    pub fn off(&mut self) {
        self.on = false;
        self.fingers.clear();
        self.held = 0;
        self.just = 0;
        self.stick = Vec2::ZERO;
        self.turn = Vec2::ZERO;
    }
}

/// The stick's push for a finger `off` its base (screen px, down is +y),
/// with reach `r`: nothing in its dead middle, full over near its rim, and
/// forward up.
pub fn stick_of(off: Vec2, r: f32) -> Vec2 {
    let len = off.length() / r;
    if len <= DEAD {
        return Vec2::ZERO;
    }
    let push = ((len - DEAD) / (FULL - DEAD)).min(1.0);
    Vec2::new(off.x, -off.y).normalize() * push
}

#[derive(Component)]
struct TouchRoot;

/// One drawn part of the controls.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Pad(Pad),
    Base,
    Knob,
}

#[derive(Component)]
struct PadWord(Pad);

/// What the layout was built for: the window, the UI's scale.
#[derive(Default, PartialEq, Clone, Copy)]
struct Built(Vec2, f32);

/// The HUD's corners a finger can press, for the layout.
type Zones<'w, 's> = Query<
    'w,
    's,
    (
        &'static ComputedNode,
        &'static UiGlobalTransform,
        Has<crate::hud::GrenBox>,
    ),
    Or<(With<crate::hud::GrenBox>, With<crate::hud::ScoreCorner>)>,
>;

/// The fingers in, before `pump` reads them.
#[allow(clippy::too_many_arguments)]
pub fn read(
    mut touch: ResMut<Touch>,
    mut fingers: MessageReader<TouchInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    ui: Res<UiScale>,
    zones: Zones,
    (screen, in_match): (Res<State<Screen>>, Option<Res<crate::front::InMatch>>),
    mut next: ResMut<NextState<Screen>>,
    mut wheel: MessageWriter<MouseWheel>,
) {
    let Ok((window, win)) = windows.single() else {
        return;
    };
    if keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some() {
        touch.off();
    }
    let playing = *screen.get() == Screen::Playing && crate::in_match(in_match);
    let live = touch.on && playing;
    let to_logical = 1.0 / win.scale_factor();
    let (mut gren, mut board): (Option<Rect>, Option<Rect>) = (None, None);
    for (n, t, is_gren) in &zones {
        let r = Rect::from_center_size(t.translation * to_logical, n.size() * to_logical);
        if r.is_empty() {
            continue;
        }
        let slot = if is_gren { &mut gren } else { &mut board };
        *slot = Some(slot.map_or(r, |s| s.union(r)));
    }
    touch.lay = layout(win.width(), win.height(), ui.0, gren, board);
    touch.begin(live);
    for f in fingers.read() {
        // The first finger down in a match plays already.
        if f.phase == TouchPhase::Started && !touch.live && playing {
            touch.on = true;
            touch.live = true;
        }
        touch.feed(f.id, f.phase, f.position);
    }
    touch.end();
    if touch.menu && *screen.get() == Screen::Playing {
        next.set(Screen::Paused);
    }
    if !touch.live && touch.scroll != 0.0 {
        wheel.write(MouseWheel {
            unit: MouseScrollUnit::Pixel,
            x: 0.0,
            y: touch.scroll,
            window,
        });
    }
}

const FACE: Color = Color::srgba(0.04, 0.07, 0.11, 0.30);
const FACE_DOWN: Color = Color::srgba(0.45, 0.7, 1.0, 0.42);
const EDGE_ON: Color = Color::srgba(0.74, 0.88, 1.0, 0.55);
const EDGE_OFF: Color = Color::srgba(0.74, 0.88, 1.0, 0.2);
const ZONE_EDGE: Color = Color::srgba(0.74, 0.88, 1.0, 0.18);
const FIRE_EDGE: Color = Color::srgba(1.0, 0.55, 0.42, 0.7);
const FIRE_DOWN: Color = Color::srgba(1.0, 0.45, 0.3, 0.4);
const WORD_OFF: Color = Color::srgba(0.74, 0.88, 1.0, 0.4);
/// A button's edge and its word, logical px; the knob's share of the
/// stick's reach.
const EDGE_W: f32 = 2.0;
const WORD_MAX: f32 = 13.0;
const KNOB: f32 = 0.42;

/// What the buttons say and whether they would do anything now.
struct Face {
    word: &'static str,
    on: bool,
    lit: bool,
}

fn face(p: Pad, s: &client_net::session::Session, seen: &crate::Seen) -> Face {
    let core = &s.core;
    let f = core.own().0;
    let at_controls = |kind: u8| {
        f.riding().is_some_and(|(i, seat)| {
            seat == sim::vehicle::DRIVER && core.map.vehicles.get(i).is_some_and(|d| d.kind == kind)
        })
    };
    let bomber = at_controls(sim::vehicle::FLIER);
    let pilot = bomber || at_controls(sim::vehicle::SKIMMER);
    let plain = Face {
        word: p.label(),
        on: true,
        lit: false,
    };
    match p {
        Pad::Crouch if pilot => Face { word: "BOOST", ..plain },
        Pad::Zoom if bomber => Face { word: "BOMB", ..plain },
        Pad::Zoom if f.dual() => Face { word: "LEFT", ..plain },
        Pad::Zoom => Face {
            on: f.riding().is_none() && sim::arms::arm(f.gun().arm).zoom_levels() > 0,
            ..plain
        },
        // Lit when there is something to take (the HUD's prompt), or a
        // ride to get out of.
        Pad::Use => Face {
            lit: s.ready && crate::items::offer(core, &seen.vehicles).is_some(),
            ..plain
        },
        _ => plain,
    }
}

fn place(n: &mut Node, at: Vec2, size: Vec2, ui: f32) {
    let (l, t) = (Val::Px((at.x - size.x * 0.5) / ui), Val::Px((at.y - size.y * 0.5) / ui));
    let (w, h) = (Val::Px(size.x / ui), Val::Px(size.y / ui));
    if n.left != l || n.top != t || n.width != w || n.height != h {
        (n.left, n.top, n.width, n.height) = (l, t, w, h);
    }
}

/// The controls drawn: built again for another window or scale, each
/// frame lit by what is held.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    touch: Res<Touch>,
    link: NonSend<crate::LinkRes>,
    seen: Res<crate::Seen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui: Res<UiScale>,
    roots: Query<Entity, With<TouchRoot>>,
    mut root_vis: Query<&mut Visibility, With<TouchRoot>>,
    mut parts: Query<(&Part, &mut Node, &mut BackgroundColor, &mut BorderColor)>,
    mut words: Query<(&PadWord, &mut Text, &mut TextColor)>,
    mut built: Local<Built>,
) {
    let Ok(win) = windows.single() else {
        return;
    };
    let s = link.0.session();
    let shown = touch.live && !s.watch;
    if let Ok(mut v) = root_vis.single_mut() {
        v.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    if !shown {
        return;
    }
    let now = Built(win.size(), ui.0);
    if *built != now || roots.is_empty() {
        *built = now;
        for e in &roots {
            commands.entity(e).despawn();
        }
        spawn(&mut commands, &touch.lay, ui.0);
        return;
    }
    let ui = ui.0;
    let lay = &touch.lay;
    let edge = Val::Px(EDGE_W / ui);
    for (part, mut n, mut bg, mut border) in &mut parts {
        match *part {
            Part::Pad(p) => {
                let i = PADS.iter().position(|q| *q == p).unwrap_or(0);
                match lay.pads[i] {
                    Some(Shape::Round { at, r }) => place(&mut n, at, Vec2::splat(2.0 * r), ui),
                    Some(Shape::Box(b)) => place(&mut n, b.center(), b.size() + ZONE_SLOP, ui),
                    None => {}
                }
                let down = touch.is_down(p);
                let f = face(p, s, &seen);
                let zone = matches!(p, Pad::GrenSwitch | Pad::Scores);
                let (fill, rim) = match (p, down) {
                    (Pad::Fire, true) => (FIRE_DOWN, FIRE_EDGE),
                    (Pad::Fire, false) => (FACE, FIRE_EDGE),
                    (_, true) => (FACE_DOWN, EDGE_ON),
                    _ if zone => (Color::NONE, ZONE_EDGE),
                    _ if f.lit => (FACE, crate::ui::MARK),
                    _ if f.on => (FACE, EDGE_ON),
                    _ => (FACE, EDGE_OFF),
                };
                bg.set_if_neq(BackgroundColor(fill));
                border.set_if_neq(BorderColor::all(rim));
                if n.border != UiRect::all(edge) && !zone {
                    n.border = UiRect::all(edge);
                }
            }
            Part::Base | Part::Knob => {
                let (o, at) = touch.stick_at().unwrap_or((lay.stick_home, lay.stick_home));
                let held = touch.stick_at().is_some();
                let off = (at - o).clamp_length_max(lay.stick_r);
                let (c, r) = if *part == Part::Base {
                    (o, lay.stick_r)
                } else {
                    (o + off, lay.stick_r * KNOB)
                };
                place(&mut n, c, Vec2::splat(2.0 * r), ui);
                let rim = if held { EDGE_ON } else { EDGE_OFF };
                border.set_if_neq(BorderColor::all(rim));
                if *part == Part::Knob {
                    bg.set_if_neq(BackgroundColor(if held { FACE_DOWN } else { FACE }));
                }
            }
        }
    }
    for (w, mut t, mut c) in &mut words {
        let f = face(w.0, s, &seen);
        if t.0 != f.word {
            t.0 = f.word.to_string();
        }
        c.set_if_neq(TextColor(if f.on { crate::hud::INK } else { WORD_OFF }));
    }
}

fn spawn(commands: &mut Commands, lay: &Layout, ui: f32) {
    let root = commands
        .spawn((
            TouchRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            // Over the HUD and the board, under the title and the menus.
            GlobalZIndex(2),
            Visibility::Hidden,
        ))
        .id();
    let round = |at: Vec2, r: f32| {
        let mut n = Node {
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(EDGE_W / ui)),
            border_radius: BorderRadius::MAX,
            ..default()
        };
        place(&mut n, at, Vec2::splat(2.0 * r), ui);
        n
    };
    for part in [Part::Base, Part::Knob] {
        let r = if part == Part::Base {
            lay.stick_r
        } else {
            lay.stick_r * KNOB
        };
        commands.spawn((
            ChildOf(root),
            part,
            round(lay.stick_home, r),
            BackgroundColor(FACE),
            BorderColor::all(EDGE_OFF),
        ));
    }
    for (i, p) in PADS.iter().enumerate() {
        let node = match lay.pads[i] {
            Some(Shape::Round { at, r }) => round(at, r),
            Some(Shape::Box(b)) => {
                let mut n = Node {
                    position_type: PositionType::Absolute,
                    border: UiRect::all(Val::Px(1.0 / ui)),
                    border_radius: BorderRadius::all(Val::Px(6.0 / ui)),
                    ..default()
                };
                place(&mut n, b.center(), b.size() + ZONE_SLOP, ui);
                n
            }
            // A zone the HUD hasn't laid out: placed when it has.
            None => Node {
                position_type: PositionType::Absolute,
                border: UiRect::all(Val::Px(1.0 / ui)),
                border_radius: BorderRadius::all(Val::Px(6.0 / ui)),
                ..default()
            },
        };
        let zone = matches!(p, Pad::GrenSwitch | Pad::Scores);
        let mut e = commands.spawn((
            ChildOf(root),
            Part::Pad(*p),
            node,
            BackgroundColor(FACE),
            BorderColor::all(EDGE_ON),
        ));
        if zone {
            continue;
        }
        // The word fits its face: a long one smaller.
        let r = match lay.pads[i] {
            Some(Shape::Round { r, .. }) => r,
            _ => MENU_R,
        };
        let size = WORD_MAX.min(1.5 * r / (p.label().len() as f32 * 0.62)).max(7.0);
        e.with_child((
            PadWord(*p),
            Text::new(p.label()),
            crate::ui::bold(size / ui),
            TextColor(crate::hud::INK),
        ));
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Touch>()
        .add_systems(Update, (draw.after(read).after(crate::see), crate::hud::score_corner));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phone() -> Touch {
        Touch {
            on: true,
            lay: layout(PHONE.x, PHONE.y, 0.54, None, None),
            ..default()
        }
    }

    fn centre(t: &Touch, p: Pad) -> Vec2 {
        match t.lay.pads[PADS.iter().position(|q| *q == p).unwrap()] {
            Some(Shape::Round { at, .. }) => at,
            _ => panic!("{p:?} isn't round"),
        }
    }

    fn frame(t: &mut Touch, fingers: &[(u64, TouchPhase, Vec2)]) {
        t.begin(true);
        for &(id, ph, at) in fingers {
            t.feed(id, ph, at);
        }
        t.end();
    }

    /// Every button is on the screen and none sits on another's face.
    #[test]
    fn the_buttons_fit_and_dont_overlap() {
        for (w, h) in [
            (844.0, 390.0),
            (667.0, 375.0),
            (932.0, 430.0),
            (1180.0, 820.0),
            (1280.0, 720.0),
        ] {
            let lay = layout(w, h, 0.5, None, None);
            let rounds: Vec<(Pad, Vec2, f32)> = PADS
                .iter()
                .zip(lay.pads)
                .filter_map(|(p, s)| match s {
                    Some(Shape::Round { at, r }) => Some((*p, at, r)),
                    _ => None,
                })
                .collect();
            for &(p, at, r) in &rounds {
                assert!(
                    at.x - r >= 0.0 && at.y - r >= 0.0 && at.x + r <= w && at.y + r <= h,
                    "{p:?} off a {w}x{h} screen"
                );
                assert!(at.x > lay.split, "{p:?} is on the stick's side");
                for &(q, bt, br) in &rounds {
                    if p != q {
                        assert!(at.distance(bt) > r + br, "{p:?} on {q:?} at {w}x{h}");
                    }
                }
            }
            let s = lay.stick_home;
            assert!(s.x - lay.stick_r >= 0.0 && s.y + lay.stick_r <= h && s.x + lay.stick_r < lay.split);
        }
    }

    /// Fire with the right thumb and turn with it, the stick under the
    /// left: three fingers, each its own.
    #[test]
    fn two_thumbs_move_turn_and_fire() {
        let mut t = phone();
        let fire = centre(&t, Pad::Fire);
        let left = Vec2::new(120.0, 280.0);
        frame(
            &mut t,
            &[(1, TouchPhase::Started, left), (2, TouchPhase::Started, fire)],
        );
        assert_ne!(t.just() & bit(Action::Fire), 0);
        assert_ne!(t.held() & bit(Action::Fire), 0);
        assert_eq!(t.stick, Vec2::ZERO, "the stick comes down where the thumb does");
        frame(
            &mut t,
            &[
                (1, TouchPhase::Moved, left + Vec2::new(0.0, -60.0)),
                (2, TouchPhase::Moved, fire + Vec2::new(12.0, 3.0)),
            ],
        );
        assert_eq!(t.just(), 0);
        assert_ne!(t.held() & bit(Action::Fire), 0);
        assert!(
            t.stick.y > 0.99 && t.stick.x.abs() < 1e-3,
            "pushed up is forward: {}",
            t.stick
        );
        assert_eq!(t.turn, Vec2::new(12.0, 3.0), "FIRE's thumb aims");
        frame(&mut t, &[(2, TouchPhase::Ended, fire)]);
        assert_eq!(t.held() & bit(Action::Fire), 0);
        assert!(t.stick.y > 0.99, "the other thumb holds");
    }

    /// A tap that comes and goes in one frame still presses, once.
    #[test]
    fn a_quick_tap_counts() {
        let mut t = phone();
        let g = centre(&t, Pad::Grenade);
        frame(&mut t, &[(5, TouchPhase::Started, g), (5, TouchPhase::Ended, g)]);
        assert_ne!(t.just() & bit(Action::Grenade), 0);
        assert_eq!(t.held(), 0);
        frame(&mut t, &[]);
        assert_eq!(t.just(), 0);
    }

    /// The right side off the buttons turns; past the stick's reach its
    /// base follows the thumb, so turning back is at once.
    #[test]
    fn the_view_and_the_stick_follow() {
        let mut t = phone();
        let look = Vec2::new(500.0, 120.0);
        frame(&mut t, &[(1, TouchPhase::Started, look)]);
        assert!(t.tapped);
        frame(&mut t, &[(1, TouchPhase::Moved, look + Vec2::new(-30.0, 10.0))]);
        assert_eq!(t.turn, Vec2::new(-30.0, 10.0));
        assert_eq!(t.held(), 0);
        let left = Vec2::new(150.0, 250.0);
        frame(&mut t, &[(2, TouchPhase::Started, left)]);
        frame(&mut t, &[(2, TouchPhase::Moved, left + Vec2::new(200.0, 0.0))]);
        assert!(t.stick.x > 0.99);
        // Its base is a reach behind the thumb: back past its rim the other way.
        let back = 200.0 - t.lay.stick_r * (1.0 + FULL) - 1.0;
        frame(&mut t, &[(2, TouchPhase::Moved, left + Vec2::new(back, 0.0))]);
        assert!(t.stick.x < -0.99, "back the other way at once: {}", t.stick);
    }

    /// Off the match nothing is held, and what was held is let go; a drag
    /// is the menu's scroll.
    #[test]
    fn the_menu_takes_the_fingers() {
        let mut t = phone();
        let fire = centre(&t, Pad::Fire);
        frame(&mut t, &[(1, TouchPhase::Started, fire)]);
        assert_ne!(t.held() & bit(Action::Fire), 0);
        t.begin(false);
        t.feed(1, TouchPhase::Moved, fire + Vec2::new(0.0, 20.0));
        t.end();
        assert_eq!(t.held(), 0);
        assert_eq!(t.scroll, 20.0);
        t.begin(true);
        t.end();
        assert_eq!(t.held(), 0, "a finger from the menu never fires");
    }

    #[test]
    fn the_stick_has_a_dead_middle_and_a_full_rim() {
        assert_eq!(stick_of(Vec2::new(5.0, 0.0), 60.0), Vec2::ZERO);
        let half = stick_of(Vec2::new(0.0, 30.0), 60.0);
        assert!(half.y < -0.3 && half.y > -0.6, "{half}");
        assert!(stick_of(Vec2::new(55.0, 0.0), 60.0).abs_diff_eq(Vec2::X, 1e-6));
    }

    #[test]
    fn the_prompts_name_the_buttons() {
        let b = Binds::default();
        let mut t = phone();
        assert_eq!(key(&b, &t, Action::Use), "USE");
        t.on = false;
        assert_eq!(key(&b, &t, Action::Use), "E");
    }
}
