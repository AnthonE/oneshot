//! The look of the game's own screens, after Gates' `client/src/render/ui.rs`
//! (MIT): its palette (warm greys, a blue selection, one saturated control),
//! its shell (the wordmark top-left, a nav column whose type is its own
//! button, a tinted panel and a wide pane over the scene and its wash) and
//! the pieces every screen is built of. The title, the Esc menu and the
//! settings are all made of these, so they read as one product.
//!
//! **The faces are ours, not Gates'** (its Roboto Condensed is Apache, and
//! our assets are CC0, CC-BY or ours). They are compiled in, not loaded from
//! `assets/`: a `Handle<Font>` that never resolves draws nothing at all, so
//! a missing file would be a client that can't say what went wrong
//! (`assets/fonts/FONTS.md`).
//!
//! - **Unitblock**: the display face. The wordmark, the HUD's numbers and
//!   banners. Tall, narrow and squared.
//! - **Aileron**: the text face, and Bevy's default here (inserted at the
//!   default handle), so any text that names no font is in it. Bold is the
//!   common case, as in Gates: every label, heading, number and button.

use bevy::asset::uuid_handle;
use bevy::prelude::*;
use bevy::text::FontWeight;

pub const DISPLAY: Handle<Font> = uuid_handle!("8a1e3f52-6c0d-4b7e-9f21-3d5c7a9e0b01");
pub const TEXT: Handle<Font> = uuid_handle!("8a1e3f52-6c0d-4b7e-9f21-3d5c7a9e0b02");
pub const TEXT_SEMI: Handle<Font> = uuid_handle!("8a1e3f52-6c0d-4b7e-9f21-3d5c7a9e0b03");
pub const TEXT_BOLD: Handle<Font> = uuid_handle!("8a1e3f52-6c0d-4b7e-9f21-3d5c7a9e0b04");

const UNITBLOCK: &[u8] = include_bytes!("../assets/fonts/Unitblock.ttf");
const AILERON: &[u8] = include_bytes!("../assets/fonts/Aileron-Regular.otf");
const AILERON_SEMI: &[u8] = include_bytes!("../assets/fonts/Aileron-SemiBold.otf");
const AILERON_BOLD: &[u8] = include_bytes!("../assets/fonts/Aileron-Bold.otf");

/// The fonts into `Assets<Font>` as the app is built (before `Startup`,
/// so the first frame's text has them), Aileron at the default handle too.
pub fn plugin(app: &mut App) {
    let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
    for (id, bytes, what) in [
        (DISPLAY.id(), UNITBLOCK, "Unitblock"),
        (TEXT.id(), AILERON, "Aileron Regular"),
        (TEXT_SEMI.id(), AILERON_SEMI, "Aileron SemiBold"),
        (TEXT_BOLD.id(), AILERON_BOLD, "Aileron Bold"),
        (AssetId::default(), AILERON, "the default"),
    ] {
        let font = Font::try_from_bytes(bytes.to_vec()).unwrap_or_else(|e| panic!("{what} is not a font: {e}"));
        fonts
            .insert(id, font)
            .unwrap_or_else(|e| panic!("{what} could not be registered: {e}"));
    }
    app.add_systems(Update, (hover, nav_hover, scale));
}

/// The display face at `size`.
pub fn display(size: f32) -> TextFont {
    TextFont {
        font: DISPLAY,
        font_size: size,
        ..default()
    }
}

/// The text face at `size`. The three Aileron weights share one family
/// name, so the weight (not the handle) is what picks the face (Gates found
/// every bold drawn regular without it).
pub fn text(size: f32) -> TextFont {
    TextFont {
        font: TEXT,
        font_size: size,
        weight: FontWeight::NORMAL,
        ..default()
    }
}

pub fn semibold(size: f32) -> TextFont {
    TextFont {
        font: TEXT_SEMI,
        font_size: size,
        weight: FontWeight::SEMIBOLD,
        ..default()
    }
}

pub fn bold(size: f32) -> TextFont {
    TextFont {
        font: TEXT_BOLD,
        font_size: size,
        weight: FontWeight::BOLD,
        ..default()
    }
}

// ---- the palette: Gates' (`render/ui.rs`, `client-web/web/index.html`) ----
//
// Warm greys (R > G > B in every one), the blue of a selection, and one
// saturated control on a screen. Ours in one place: the mark and the PLAY
// button are oneshot's gold where Gates has its sigil's green.

/// The screen behind everything: darker than any panel.
pub const BG: Color = Color::srgb(0.106, 0.098, 0.086);
/// A panel on the background: the settings' rail and pane off a match.
pub const PANEL: Color = Color::srgba(0.259, 0.243, 0.224, 1.0);
/// A clickable row at rest, and under the pointer.
pub const ROW_IDLE: Color = Color::srgba(0.169, 0.153, 0.137, 0.92);
pub const ROW_HOVER: Color = Color::srgba(0.278, 0.263, 0.235, 1.0);
/// The selection: the one cool hue on a warm panel.
pub const ACCENT: Color = Color::srgb(0.224, 0.510, 0.729);
pub const ACCENT_HOVER: Color = Color::srgb(0.290, 0.580, 0.800);
/// A selected row in a list: the accent, see-through.
pub const PICKED: Color = Color::srgba(0.224, 0.510, 0.729, 0.32);
/// Hairlines.
pub const RULE: Color = Color::srgba(0.75, 0.72, 0.62, 0.20);
pub const RULE_HI: Color = Color::srgba(0.75, 0.72, 0.62, 0.34);
/// The shell's grounds: see-through, so the scene moves behind them.
pub const PANEL_BG: Color = Color::srgba(0.118, 0.106, 0.094, 0.84);
pub const STAGE_BG: Color = Color::srgba(0.078, 0.071, 0.063, 0.68);
/// The settings over a live match: heavier, the type has a moving scene
/// behind it.
pub const PANE_BG: Color = Color::srgba(0.086, 0.078, 0.070, 0.86);
/// The front screen's one saturated colour: the nav's bar, the PLAY
/// button. Oneshot's gold (Gates': `#86b834`).
pub const MARK: Color = Color::srgb(0.91, 0.725, 0.192);
pub const GO_HOVER: Color = Color::srgb(0.96, 0.8, 0.33);
pub const GO_INK: Color = Color::srgb(0.067, 0.059, 0.031);
/// PLAY with nothing to do yet (connecting, waiting in a queue).
pub const GO_WAIT: Color = Color::srgb(0.40, 0.33, 0.13);
/// A sunken well: a text field, the identity block.
pub const WELL: Color = Color::srgba(0.0, 0.0, 0.0, 0.30);
/// A nav entry at rest: dimmer than `DIM`, so the picked one is obvious.
pub const NAV_IDLE: Color = Color::srgba(0.67, 0.65, 0.61, 0.62);
/// Type, in four weights of attention.
pub const TITLE: Color = Color::srgb(0.93, 0.92, 0.90);
pub const TEXT_COLOR: Color = Color::srgb(0.92, 0.90, 0.85);
pub const DIM: Color = Color::srgba(0.74, 0.72, 0.68, 0.86);
pub const FAINT: Color = Color::srgba(0.62, 0.60, 0.56, 0.80);
/// A line that went wrong, or right.
pub const BAD: Color = Color::srgb(0.89, 0.54, 0.42);
pub const GOOD: Color = Color::srgb(0.65, 0.81, 0.43);
/// Elo Pros' own lime: a signed-in wallet.
pub const ELO: Color = Color::srgb(0.72, 1.0, 0.22);

/// A button's background at rest and under the pointer: carried per
/// entity, so a selected row isn't repainted on the way past it.
#[derive(Component, Clone, Copy)]
pub struct Hover {
    pub idle: Color,
    pub over: Color,
}

impl Hover {
    pub fn new(idle: Color, over: Color) -> Self {
        Hover { idle, over }
    }
}

impl Default for Hover {
    fn default() -> Self {
        Hover::new(ROW_IDLE, ROW_HOVER)
    }
}

/// How far a pressed button sinks.
const PRESS_SCALE: f32 = 0.96;

/// A lit colour, pressed: a fifth darker, same alpha.
pub fn pressed(c: Color) -> Color {
    let s = c.to_srgba();
    Color::srgba(s.red * 0.8, s.green * 0.8, s.blue * 0.8, s.alpha)
}

/// One hover for every button: lit under the pointer, a shade down and
/// sunk a hair while pressed.
fn hover(mut q: Query<(&Interaction, &Hover, &mut BackgroundColor, &mut UiTransform), Changed<Interaction>>) {
    for (i, h, mut bg, mut xf) in &mut q {
        let (want, scale) = match i {
            Interaction::None => (h.idle, 1.0),
            Interaction::Hovered => (h.over, 1.0),
            Interaction::Pressed => (pressed(h.over), PRESS_SCALE),
        };
        bg.set_if_neq(BackgroundColor(want));
        if xf.scale != Vec2::splat(scale) {
            xf.scale = Vec2::splat(scale);
        }
    }
}

/// The UI's scale: one at 1280 by 720 and up, smaller in a smaller
/// window, so the shell keeps its shape.
fn scale(windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut ui: ResMut<UiScale>) {
    let Ok(w) = windows.single() else {
        return;
    };
    let s = (w.width() / 1280.0).min(w.height() / 720.0).clamp(0.35, 1.0);
    if (ui.0 - s).abs() > 1e-3 {
        ui.0 = s;
    }
}

/// Bold type.
pub fn strong(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), bold(size), TextColor(color))
}

/// Plain type: prose, a hint, a status line.
pub fn label(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), text(size), TextColor(color))
}

/// The shadow floating type carries over the scene.
pub const TEXT_SHADOW: TextShadow = TextShadow {
    offset: Vec2::new(1.0, 2.0),
    color: Color::srgba(0.0, 0.0, 0.0, 0.6),
};

/// A full-screen column, centred: the root of the Esc menu.
pub fn screen(bg: Color) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(10.0),
            ..default()
        },
        BackgroundColor(bg),
        GlobalZIndex(10),
    )
}

/// The full-screen root of a shell screen (the title, the settings): a
/// column, the wordmark's row on top.
pub fn shell() -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        ..default()
    }
}

/// The wash over the scene, Gates' (`ui::scrim`, the page's `.wash` and
/// `.vignette`): heavier on the left where the nav and the panel sit and
/// at the foot, so the map shows on the right.
pub fn scrim() -> impl Bundle {
    let ink = |a: f32| Color::srgba(0.055, 0.051, 0.043, a);
    let stops = |s: &[(f32, f32)]| {
        s.iter()
            .map(|&(a, at)| ColorStop::new(ink(a), Val::Percent(at)))
            .collect::<Vec<_>>()
    };
    (
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundGradient(vec![
            LinearGradient::to_right(stops(&[(0.88, 0.0), (0.62, 24.0), (0.18, 56.0), (0.04, 100.0)])).into(),
            LinearGradient::to_bottom(stops(&[(0.45, 0.0), (0.0, 18.0), (0.0, 52.0), (0.84, 100.0)])).into(),
            RadialGradient::new(
                UiPosition::TOP_LEFT.at_percent(64.0, 40.0),
                RadialGradientShape::FarthestCorner,
                vec![
                    ColorStop::new(Color::NONE, Val::Percent(45.0)),
                    ColorStop::new(Color::srgba(0.0, 0.0, 0.0, 0.45), Val::Percent(100.0)),
                ],
            )
            .into(),
        ]),
        ZIndex(-1),
    )
}

/// The wordmark, top-left: the word in the display face, a gold rule under
/// it, and a line in small capitals.
pub fn wordmark(line: &str) -> impl Bundle {
    wordmark_at(line, AlignItems::FlexStart)
}

/// The wordmark, its two lines aligned as `align` says (centred on the Esc
/// menu).
pub fn wordmark_at(line: &str, align: AlignItems) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            align_items: align,
            row_gap: Val::Px(7.0),
            ..default()
        },
        children![
            (Text::new("ONESHOT"), display(46.0), TextColor(TITLE), TEXT_SHADOW),
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                children![
                    (
                        Node {
                            width: Val::Px(26.0),
                            height: Val::Px(3.0),
                            ..default()
                        },
                        BackgroundColor(MARK),
                    ),
                    strong(line.to_string(), 11.0, FAINT),
                ],
            ),
        ],
    )
}

/// A nav entry's label, for [`nav_hover`].
#[derive(Component, Clone, Copy)]
pub struct NavItem {
    pub picked: bool,
}

/// The nav label inside a [`NavItem`].
#[derive(Component)]
pub struct NavLabel;

/// One entry in the nav column: dim at rest, bright when picked, with the
/// mark's bar out in the gutter. The type is the button; no chrome.
pub fn nav_item(text: &str, picked: bool, size: f32) -> impl Bundle {
    (
        Button,
        NavItem { picked },
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(0.0), Val::Px(7.0)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        children![
            (
                Node {
                    width: Val::Px(5.0),
                    height: Val::Px(size * 0.85),
                    margin: UiRect::new(Val::Px(-15.0), Val::Px(10.0), Val::Px(0.0), Val::Px(0.0)),
                    ..default()
                },
                BackgroundColor(if picked { MARK } else { Color::NONE }),
            ),
            (
                strong(text.to_string(), size, nav_color(picked, false)),
                NavLabel,
                TEXT_SHADOW
            ),
        ],
    )
}

/// A nav label's colour: picked is brightest, the pointer lifts the rest.
pub fn nav_color(picked: bool, over: bool) -> Color {
    match (picked, over) {
        (true, _) => TITLE,
        (false, true) => TEXT_COLOR,
        (false, false) => NAV_IDLE,
    }
}

/// A nav entry under the pointer: its label lifts and it steps right, as
/// the page's `.nav .item:hover`.
fn nav_hover(
    mut items: Query<(&Interaction, &NavItem, &Children, &mut UiTransform), Changed<Interaction>>,
    mut labels: Query<&mut TextColor, With<NavLabel>>,
) {
    for (i, n, kids, mut xf) in &mut items {
        let over = *i != Interaction::None;
        for k in kids.iter() {
            if let Ok(mut c) = labels.get_mut(k) {
                c.set_if_neq(TextColor(nav_color(n.picked, over)));
            }
        }
        let dx = if over && !n.picked { 4.0 } else { 0.0 };
        xf.translation = Val2::px(dx, 0.0);
    }
}

/// The tinted control column beside the nav.
pub fn panel(width: f32) -> impl Bundle {
    (
        Node {
            width: Val::Px(width),
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            row_gap: Val::Px(7.0),
            padding: UiRect::all(Val::Px(18.0)),
            border: UiRect::all(Val::Px(1.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(RULE),
    )
}

/// A panel's heading.
pub fn heading(s: &str) -> impl Bundle {
    strong(s.to_string(), 26.0, TITLE)
}

/// A group heading with a rule running out to its right (`.group::after`).
pub fn group(s: &str) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            margin: UiRect::top(Val::Px(8.0)),
            flex_shrink: 0.0,
            ..default()
        },
        children![
            strong(s.to_string(), 12.0, FAINT),
            (
                Node {
                    flex_grow: 1.0,
                    height: Val::Px(1.0),
                    ..default()
                },
                BackgroundColor(RULE),
            ),
        ],
    )
}

/// Empty, and it eats whatever space is left.
pub fn spacer() -> Node {
    Node {
        flex_grow: 1.0,
        ..default()
    }
}

/// The PLAY button: the one saturated control on the screen. `ready`
/// false: nothing to do yet (the page's `.go.wait`).
pub fn go(ready: bool) -> impl Bundle {
    let (idle, over) = if ready { (MARK, GO_HOVER) } else { (GO_WAIT, GO_WAIT) };
    (
        Button,
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(58.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(5.0),
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(idle),
        Hover::new(idle, over),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.35),
            Val::Px(0.0),
            Val::Px(10.0),
            Val::Px(0.0),
            Val::Px(30.0),
        ),
    )
}

/// The ink on [`go`]: dark on the gold, light on the waiting olive.
pub fn go_ink(ready: bool) -> Color {
    if ready {
        GO_INK
    } else {
        Color::srgba(0.95, 0.93, 0.86, 0.9)
    }
}

/// How tall a flat button is.
pub const BUTTON_H: f32 = 36.0;

/// A flat button, `width` px wide; `accent`: the blue one.
pub fn button(width: f32, accent: bool) -> impl Bundle {
    let (idle, over) = if accent {
        (ACCENT, ACCENT_HOVER)
    } else {
        (ROW_IDLE, ROW_HOVER)
    };
    (
        Button,
        Node {
            width: Val::Px(width),
            height: Val::Px(BUTTON_H),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(idle),
        BorderColor::all(if accent { Color::NONE } else { RULE }),
        Hover::new(idle, over),
    )
}

/// A flat button's label: small bold capitals.
pub fn button_label(s: impl Into<String>) -> impl Bundle {
    strong(s, 13.0, TITLE)
}

/// The small square `-` and `+` either side of a number.
pub fn stepper() -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(30.0),
            height: Val::Px(26.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(ROW_IDLE),
        BorderColor::all(RULE),
        Hover::default(),
    )
}

/// A wide clickable row: the Esc menu's verbs, a mode in the play panel.
pub fn row(width: Val, picked: bool) -> impl Bundle {
    let (idle, over) = if picked {
        (PICKED, Color::srgba(0.290, 0.580, 0.800, 0.42))
    } else {
        (ROW_IDLE, ROW_HOVER)
    };
    (
        Button,
        Node {
            width,
            padding: UiRect::axes(Val::Px(14.0), Val::Px(9.0)),
            border: UiRect::new(
                Val::Px(if picked { 3.0 } else { 1.0 }),
                Val::Px(1.0),
                Val::Px(1.0),
                Val::Px(1.0),
            ),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(idle),
        BorderColor {
            left: if picked { ACCENT_HOVER } else { RULE },
            ..BorderColor::all(RULE)
        },
        Hover::new(idle, over),
    )
}

/// Despawns every entity carrying `M`: a screen's teardown.
pub fn despawn_all<M: Component>(mut commands: Commands, roots: Query<Entity, With<M>>) {
    for e in &roots {
        commands.entity(e).despawn();
    }
}

/// The digit keys numbered rows answer to.
pub const DIGITS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// Which of the first `rows` numbered rows' digits was pressed, if any.
pub fn digit_pressed(keys: &ButtonInput<KeyCode>, rows: usize) -> Option<usize> {
    DIGITS.iter().take(rows).position(|k| keys.just_pressed(*k))
}
