//! The look of the game's own screens and text, after Gates'
//! `client/src/render/ui.rs` (MIT): the fonts, the palette, and the few
//! pieces every menu is built of.
//!
//! **The fonts are compiled in**, not loaded from `assets/`: a `Handle<Font>`
//! that never resolves draws nothing at all, so a missing file would be a
//! client that can't even say what went wrong. Both are CC0
//! (`assets/fonts/FONTS.md`); the page's menu uses the same files.
//!
//! - **Unitblock**: the display face. The title, the HUD's numbers and
//!   banners. Tall, narrow and squared.
//! - **Aileron**: the text face, and Bevy's default here (inserted at the
//!   default handle), so any text that names no font is in it.

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
    app.add_systems(Update, hover);
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

// The palette: the page menu's dashboard (`web/index.html`): near-black
// green glass, pale green text, the lit green accent.
pub const SHADE: Color = Color::srgba(0.012, 0.035, 0.012, 0.84);
pub const PANEL: Color = Color::srgba(0.05, 0.12, 0.045, 0.95);
pub const ROW: Color = Color::srgba(0.62, 1.0, 0.24, 0.06);
pub const ROW_HOVER: Color = Color::srgba(0.62, 1.0, 0.24, 0.16);
pub const RULE: Color = Color::srgba(0.67, 1.0, 0.43, 0.32);
pub const ACCENT: Color = Color::srgb(0.616, 1.0, 0.235);
pub const ACCENT_HOVER: Color = Color::srgb(0.79, 1.0, 0.54);
pub const INK: Color = Color::srgb(0.012, 0.024, 0.012);
pub const TITLE: Color = Color::srgb(0.914, 1.0, 0.85);
pub const TEXT_COLOR: Color = Color::srgba(0.914, 1.0, 0.85, 0.92);
pub const DIM: Color = Color::srgba(0.62, 0.75, 0.55, 0.9);
/// Gold: the best medals, whatever the menus' colour.
pub const GOLD: Color = Color::srgb(0.91, 0.725, 0.192);

/// A button's background at rest and under the pointer.
#[derive(Component, Clone, Copy)]
pub struct Hover {
    pub idle: Color,
    pub over: Color,
}

/// How far a pressed button sinks.
const PRESS_SCALE: f32 = 0.96;

/// One hover for every button: lit under the pointer, a shade down and
/// sunk a hair while pressed.
fn hover(mut q: Query<(&Interaction, &Hover, &mut BackgroundColor, &mut UiTransform), Changed<Interaction>>) {
    for (i, h, mut bg, mut xf) in &mut q {
        let (want, scale) = match i {
            Interaction::None => (h.idle, 1.0),
            Interaction::Hovered => (h.over, 1.0),
            Interaction::Pressed => {
                let s = h.over.to_srgba();
                (
                    Color::srgba(s.red * 0.8, s.green * 0.8, s.blue * 0.8, s.alpha),
                    PRESS_SCALE,
                )
            }
        };
        bg.set_if_neq(BackgroundColor(want));
        if xf.scale != Vec2::splat(scale) {
            xf.scale = Vec2::splat(scale);
        }
    }
}

/// A button `width` px wide; `accent`: the gold one.
pub fn button(width: f32, accent: bool) -> impl Bundle {
    let (idle, over) = if accent {
        (ACCENT, ACCENT_HOVER)
    } else {
        (ROW, ROW_HOVER)
    };
    (
        Button,
        Node {
            width: Val::Px(width),
            height: Val::Px(40.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(idle),
        BorderColor::all(if accent { Color::NONE } else { RULE }),
        Hover { idle, over },
    )
}

/// A button's label: dark on the gold, light on the rest.
pub fn button_label(s: impl Into<String>, accent: bool) -> impl Bundle {
    (
        Text::new(s),
        bold(17.0),
        TextColor(if accent { INK } else { TEXT_COLOR }),
    )
}

/// The small square `-` and `+` either side of a number.
pub fn stepper() -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(32.0),
            height: Val::Px(28.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(ROW),
        BorderColor::all(RULE),
        Hover {
            idle: ROW,
            over: ROW_HOVER,
        },
    )
}

/// Plain text in the text face.
pub fn label(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), text(size), TextColor(color))
}

/// Despawns every entity carrying `M`: a screen's teardown.
pub fn despawn_all<M: Component>(mut commands: Commands, roots: Query<Entity, With<M>>) {
    for e in &roots {
        commands.entity(e).despawn();
    }
}
