//! The HUD, after `web/src/hud.ts`: shield and health bars, the gun and
//! its rounds, a crosshair, a hit marker on our hits (red on a kill), an
//! arc toward whoever hurt us, the score and clock, the kill feed, and a
//! line in the middle for the countdown, the result, the kill cam and the
//! respawn. Everything here is read from the session; nothing is decided.

use std::collections::BTreeMap;

use bevy::prelude::*;
use proto::{EventState, Msg};
use sim::consts::{HEALTH_MAX, SHIELD_MAX, TICK_HZ};
use sim::events::Kind;

use client_net::session::Session;

const FEED_S: f32 = 5.0;
const FEED_LINES: usize = 5;
const HIT_S: f32 = 0.22;
const DAMAGE_S: f32 = 0.6;
/// The red rim when we're hit (after Gates' `hurt_flash.rs`): its alpha at
/// the blow, how long it fades, and where the red starts (a share of the
/// far-corner radius). Faint, at the edge, gone in half a second.
const RIM_ALPHA: f32 = 0.38;
const RIM_S: f32 = 0.5;
const RIM_CLEAR_PCT: f32 = 52.0;
/// How far out from the middle the damage arc sits, px.
const DAMAGE_R: f32 = 90.0;

/// Names by slot (from `Msg::Seat`), the feed, and the markers' clocks.
#[derive(Resource, Default)]
pub struct HudState {
    names: BTreeMap<u8, String>,
    feed: Vec<(String, f32)>,
    hit_at: f32,
    hit_kill: bool,
    hurt_at: f32,
    hurt_angle: f32,
    /// Our kills and deaths as the stream told them (the page's stats).
    pub kills: u32,
    pub deaths: u32,
}

impl HudState {
    pub fn name(&self, slot: u8) -> String {
        self.names
            .get(&slot)
            .cloned()
            .unwrap_or_else(|| format!("P{}", slot + 1))
    }
}

#[derive(Component)]
pub struct ShieldBar;
#[derive(Component)]
pub struct HurtRim;
#[derive(Component)]
pub struct HealthBar;
#[derive(Component)]
pub struct AmmoText;
#[derive(Component)]
pub struct ScoreText;
#[derive(Component)]
pub struct FeedText;
#[derive(Component)]
pub struct CenterText;
#[derive(Component)]
pub struct Reticle;
#[derive(Component)]
pub struct HitMark;
#[derive(Component)]
pub struct HurtArc;
#[derive(Component)]
pub struct HitStroke;

fn text(size: f32) -> impl Bundle {
    (
        Text::new(""),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.92)),
        TextShadow::default(),
    )
}

fn bar(color: Color, marker: impl Component, bottom: f32) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(18.0),
            bottom: Val::Px(bottom),
            width: Val::Px(220.0),
            height: Val::Px(10.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        children![(
            marker,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(color),
        )],
    )
}

pub fn setup(mut commands: Commands) {
    let abs = |left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>| Node {
        position_type: PositionType::Absolute,
        left: left.map_or(Val::Auto, Val::Px),
        right: right.map_or(Val::Auto, Val::Px),
        top: top.map_or(Val::Auto, Val::Px),
        bottom: bottom.map_or(Val::Auto, Val::Px),
        ..default()
    };
    // Under the HUD, over the world.
    commands.spawn((
        HurtRim,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundGradient(vec![Gradient::Radial(RadialGradient::new(
            UiPosition::CENTER,
            RadialGradientShape::FarthestCorner,
            vec![
                ColorStop::new(Color::NONE, Val::Percent(RIM_CLEAR_PCT)),
                ColorStop::new(rim(0.0), Val::Percent(100.0)),
            ],
        ))]),
        ZIndex(-1),
        Visibility::Hidden,
    ));
    commands.spawn(bar(Color::srgb(0.35, 0.7, 1.0), ShieldBar, 40.0));
    commands.spawn(bar(Color::srgb(0.9, 0.3, 0.25), HealthBar, 24.0));
    commands.spawn((AmmoText, text(20.0), abs(None, Some(22.0), None, Some(20.0))));
    commands.spawn((ScoreText, text(18.0), abs(Some(18.0), None, Some(14.0), None)));
    commands.spawn((FeedText, text(16.0), abs(None, Some(18.0), Some(14.0), None)));
    // The middle: a full-width row, its text centred.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            top: Val::Percent(30.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(CenterText, text(26.0))],
    ));
    // The crosshair, the hit marker and the damage arc, all centred on the
    // screen's middle.
    let centred = |w: f32, h: f32| Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        top: Val::Percent(50.0),
        width: Val::Px(w),
        height: Val::Px(h),
        margin: UiRect {
            left: Val::Px(-w / 2.0),
            top: Val::Px(-h / 2.0),
            ..default()
        },
        ..default()
    };
    commands.spawn((Reticle, centred(22.0, 22.0))).with_children(|c| {
        let white = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85));
        for (l, t, w, h) in [
            (0.0, 10.0, 7.0, 2.0),
            (15.0, 10.0, 7.0, 2.0),
            (10.0, 0.0, 2.0, 7.0),
            (10.0, 15.0, 2.0, 7.0),
        ] {
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(l),
                    top: Val::Px(t),
                    width: Val::Px(w),
                    height: Val::Px(h),
                    ..default()
                },
                white,
            ));
        }
    });
    commands.spawn((
        HitMark,
        centred(26.0, 26.0),
        UiTransform::from_rotation(Rot2::degrees(45.0)),
        Visibility::Hidden,
        children![
            (
                HitStroke,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(12.0),
                    width: Val::Px(26.0),
                    height: Val::Px(2.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ),
            (
                HitStroke,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(12.0),
                    top: Val::Px(0.0),
                    width: Val::Px(2.0),
                    height: Val::Px(26.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ),
        ],
    ));
    commands.spawn((
        HurtArc,
        centred(56.0, 6.0),
        BackgroundColor(Color::srgba(1.0, 0.2, 0.15, 0.0)),
    ));
}

/// This frame's events and stream messages: the markers, names, the feed.
pub fn on_events(st: &mut HudState, s: &Session, now_ms: f64, events: &[EventState], msgs: &[Msg], now: f32, yaw: f32) {
    let me = s.core.slot;
    for e in events {
        if e.kind == Kind::Hit as u8 {
            if e.a == me && e.b != me {
                st.hit_at = now;
                st.hit_kill = false;
            }
            if e.b == me && e.a != me {
                // Where it came from, relative to where we look: from the
                // shot's start, which is the shooter's eye.
                let (_, p) = s.core.own();
                let from = s.core.others(now_ms).into_iter().find(|d| d.slot == e.a).map(|d| d.pos);
                let from = from.unwrap_or(e.pos);
                st.hurt_angle = (from.z - p.z).atan2(from.x - p.x) - yaw;
                st.hurt_at = now;
            }
        } else if e.kind == Kind::Kill as u8 && e.a == me && e.b != me {
            st.hit_at = now;
            st.hit_kill = true;
        }
    }
    for m in msgs {
        match m {
            Msg::Seat { slot, name, .. } => {
                st.names.insert(*slot, name.as_str().to_string());
            }
            Msg::Kill { killer, victim, .. } => {
                let line = if killer == victim {
                    format!("{} died", st.name(*victim))
                } else {
                    format!("{} > {}", st.name(*killer), st.name(*victim))
                };
                st.feed.push((line, now));
                if *killer == me && *victim != me {
                    st.kills += 1;
                }
                if *victim == me {
                    st.deaths += 1;
                }
            }
            _ => {}
        }
    }
    st.feed.retain(|(_, t)| now - t < FEED_S);
    while st.feed.len() > FEED_LINES {
        st.feed.remove(0);
    }
}

fn mmss(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The widgets from the session.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn draw(
    link: NonSend<crate::LinkRes>,
    look: Res<crate::Look>,
    time: Res<Time>,
    st: Res<HudState>,
    mut shield: Query<&mut Node, With<ShieldBar>>,
    mut health: Query<&mut Node, (With<HealthBar>, Without<ShieldBar>)>,
    mut arc: Query<
        (&mut Node, &mut UiTransform, &mut BackgroundColor),
        (With<HurtArc>, Without<ShieldBar>, Without<HealthBar>),
    >,
    mut texts: Query<(
        &mut Text,
        Option<&AmmoText>,
        Option<&ScoreText>,
        Option<&FeedText>,
        Option<&CenterText>,
    )>,
    mut reticle: Query<&mut Visibility, With<Reticle>>,
    mut hitmark: Query<&mut Visibility, (With<HitMark>, Without<Reticle>)>,
    mut strokes: Query<&mut BackgroundColor, (With<HitStroke>, Without<HurtArc>)>,
) {
    let s = link.0.session();
    let now = time.elapsed_secs();
    let core = &s.core;
    let (f, _) = core.own();
    if !s.ready {
        for (mut t, _, sc, _, _) in texts.iter_mut() {
            if sc.is_some() {
                t.0 = "connecting...".into();
            }
        }
        return;
    }
    if let Ok(mut n) = shield.single_mut() {
        n.width = Val::Percent((f.shield as f32 / SHIELD_MAX as f32 * 100.0).clamp(0.0, 100.0));
    }
    if let Ok(mut n) = health.single_mut() {
        n.width = Val::Percent((f.health as f32 / HEALTH_MAX as f32 * 100.0).clamp(0.0, 100.0));
    }
    let killcam = if f.alive { None } else { core.killcam(link.0.now_ms()) };
    let snap = core.latest();
    let limit = core.rules.score_limit;
    let scores = &snap.score[..snap.n_score as usize];
    let map = sim::maps::ALL.iter().find(|d| d.id == s.map_id).map_or("", |d| d.name);
    let score = if core.rules.teams {
        let side = |t: usize| scores.get(t).copied().unwrap_or(0);
        let (us, them) = (side(core.team as usize & 1), side(1 - (core.team as usize & 1)));
        format!("{us} - {them}   to {limit}")
    } else {
        let mine = scores.get(core.slot as usize).copied().unwrap_or(0);
        let best = scores
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != core.slot as usize)
            .map(|(_, v)| *v)
            .max();
        format!("{mine} / {limit}   best other {}", best.unwrap_or(0))
    };
    let clock = snap.clock as f32 / TICK_HZ as f32;
    let center = match snap.phase {
        0 => format!("Starting in {}", clock.ceil() as u32),
        2 => {
            let w = snap.winner;
            let mut c = if w == 255 {
                "Draw".to_string()
            } else if core.rules.teams {
                if w & 1 == core.team & 1 {
                    "Your side wins".into()
                } else {
                    "Your side lost".into()
                }
            } else if w == core.slot {
                "You win".into()
            } else {
                format!("{} wins", st.name(w))
            };
            match core.verified {
                Some(true) => c.push_str("  -  match verified"),
                Some(false) => c.push_str("  -  verification FAILED"),
                None => {}
            }
            c
        }
        _ if !f.alive => {
            let back = (f.respawn as f32 / TICK_HZ as f32).ceil() as u32;
            match &killcam {
                Some(k) => format!("Killed by {}  -  respawning in {back}", st.name(k.killer)),
                None => format!("Respawning in {back}"),
            }
        }
        _ => String::new(),
    };
    let g = f.gun();
    let arm = sim::arms::arm(g.arm);
    let other = f.guns[1 - f.cur as usize];
    let reload = if f.reload > 0 { "  reloading" } else { "" };
    let swap = if other.arm != 0 {
        format!("\n{}", sim::arms::arm(other.arm).name)
    } else {
        String::new()
    };
    let ride = match f.riding() {
        Some((_, 0)) => "\ndriving: E gets out",
        Some(_) => "\nriding: E gets out",
        None => "",
    };
    let ammo = format!(
        "{}{reload}\n{} / {}   grenades {}{swap}{ride}",
        arm.name, g.mag, g.reserve, f.grenades
    );
    let feed: String = st.feed.iter().map(|(l, _)| format!("{l}\n")).collect();
    for (mut t, a, sc, fe, ce) in texts.iter_mut() {
        let v = if a.is_some() {
            ammo.clone()
        } else if sc.is_some() {
            format!("{map}\n{score}\n{}", mmss(clock))
        } else if fe.is_some() {
            feed.clone()
        } else if ce.is_some() {
            center.clone()
        } else {
            continue;
        };
        if t.0 != v {
            t.0 = v;
        }
    }
    let scoped = look.zoom && arm.zoom > 1;
    if let Ok(mut v) = reticle.single_mut() {
        *v = if scoped || !f.alive {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    let hit_u = (now - st.hit_at) / HIT_S;
    if let Ok(mut v) = hitmark.single_mut() {
        *v = if (0.0..1.0).contains(&hit_u) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let c = if st.hit_kill {
        Color::srgb(1.0, 0.25, 0.2)
    } else {
        Color::WHITE
    };
    for mut bg in strokes.iter_mut() {
        bg.0 = c;
    }
    if let Ok((mut n, mut tr, mut bg)) = arc.single_mut() {
        let u = (now - st.hurt_at) / DAMAGE_S;
        let a = st.hurt_angle;
        n.margin = UiRect {
            left: Val::Px(DAMAGE_R * a.sin() - 28.0),
            top: Val::Px(-DAMAGE_R * a.cos() - 3.0),
            ..default()
        };
        tr.rotation = Rot2::radians(a);
        let alpha = if (0.0..1.0).contains(&u) { 0.85 * (1.0 - u) } else { 0.0 };
        bg.0 = Color::srgba(1.0, 0.2, 0.15, alpha);
    }
}

fn rim(alpha: f32) -> Color {
    Color::srgba(0.55, 0.02, 0.02, alpha)
}

/// The red rim: raised by a hit on us (`HudState::hurt_at`), fading out.
pub fn hurt_rim(
    time: Res<Time>,
    st: Res<HudState>,
    mut q: Query<(&mut BackgroundGradient, &mut Visibility), With<HurtRim>>,
) {
    let Ok((mut bg, mut vis)) = q.single_mut() else {
        return;
    };
    let u = (time.elapsed_secs() - st.hurt_at) / RIM_S;
    let alpha = if st.hurt_at > 0.0 && (0.0..1.0).contains(&u) {
        RIM_ALPHA * (1.0 - u)
    } else {
        0.0
    };
    let want = if alpha > 0.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *vis != want {
        *vis = want;
    }
    if alpha > 0.0 {
        if let Some(Gradient::Radial(r)) = bg.0.first_mut() {
            if let Some(stop) = r.stops.last_mut() {
                stop.color = rim(alpha);
            }
        }
    }
}
