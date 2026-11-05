//! The party, its friends and its voice, in the game (proto's `party`, the
//! server's `party.rs` and `lobby.rs`): a lobby session opened at the title
//! and kept through every match.
//!
//! PARTY and FRIENDS on the title draw what it holds (`front.rs`). In a
//! party the leader's PLAY starts the match for everyone present: the
//! server says go to each, and each joins with the party's code, so the
//! server seats them together. Voice is the page's in a browser (the mic and
//! the speakers are a page's to ask for: `web/voice.js`): each member's
//! frame goes there to be played, ours comes back through `voice_up`. The
//! desktop has the party and no voice yet.

use bevy::prelude::*;
use proto::party::{self as pp, Member, Party};
use proto::{Msg, Pass};

use crate::door::{Ask, Door, LobbyBox, LobbyKnock};
use crate::front::InMatch;
use crate::settings::Player;

/// How long a member stays lit after their last frame.
pub const TALKING_S: f32 = 0.35;
/// The wait before opening the lobby again, doubling to the last.
const RETRY_S: [f32; 5] = [1.0, 2.0, 4.0, 8.0, 15.0];
/// An attempt that hasn't answered in this long is let go and made again.
const KNOCK_GIVE_UP_S: f32 = 20.0;
/// The lobby opens once the title's level is in (a page loading it can
/// stall long enough for the server to give up on a hello), or after this.
const FIRST_AFTER_S: f32 = 30.0;

/// One friend as the title lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Friend {
    pub name: String,
    pub player: String,
    /// `party::AT_*`.
    pub at: u8,
    /// Their party, if we may join it.
    pub party: u32,
    pub size: u8,
    pub playing: u8,
}

/// Where our microphone stands (the page's, `web/voice.js`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mic {
    /// No voice here (the desktop, or a browser without WebCodecs).
    #[default]
    None,
    Off,
    On,
    Talking,
}

/// The party as the title draws it.
#[derive(Resource, Default)]
pub struct PartyView {
    pub connected: bool,
    pub party: Option<Party>,
    pub friends: Vec<Friend>,
    /// Elo Pros answered the last time we asked for friends.
    pub reachable: bool,
    /// A friends list has come at all (we're signed in and it was read).
    pub friends_known: bool,
    friends_next: Vec<Option<Friend>>,
    friends_n: usize,
    /// The server's ticket: who we are, on every hello after this one.
    pub ticket: String,
    /// The last invite: who, and their party's code.
    pub invite: Option<(String, u32)>,
    /// The last thing the party said no to.
    pub notice: Option<String>,
    pub mic: Mic,
    /// Hold V to talk, or open mic.
    pub open_mic: bool,
    /// PLAY puts the party alone in a room of its own, bots in the rest.
    pub alone: bool,
    /// Bumped when anything the title draws moved.
    pub gen: u32,
    /// Each member's last voice frame (s since start), by tag; negative:
    /// their last frame said they stopped.
    heard: [f32; 16],
}

impl PartyView {
    pub fn members(&self) -> &[Member] {
        self.party.as_ref().map_or(&[], |p| p.members())
    }

    pub fn size(&self) -> usize {
        self.members().len().max(1)
    }

    /// We lead (or are alone).
    pub fn leads(&self) -> bool {
        self.party.as_ref().is_none_or(|p| p.leader == p.me)
    }

    pub fn me(&self) -> Option<&Member> {
        let p = self.party.as_ref()?;
        p.members().iter().find(|m| m.tag == p.me)
    }

    pub fn leader(&self) -> Option<&Member> {
        let p = self.party.as_ref()?;
        p.members().iter().find(|m| m.tag == p.leader)
    }

    /// Whether member `tag` is talking at `now`.
    pub fn talking(&self, tag: u8, now: f32) -> bool {
        if self.party.as_ref().is_some_and(|p| p.me == tag) {
            return self.mic == Mic::Talking;
        }
        let t = self.heard[(tag & 15) as usize];
        t > 0.0 && now - t < TALKING_S
    }

    pub fn code_text(&self) -> String {
        self.party.as_ref().map_or(String::new(), |p| pp::code_text(p.code))
    }

    fn touch(&mut self) {
        self.gen = self.gen.wrapping_add(1);
    }
}

/// A member's key for mutes: their Elo Pros id, or a guest's name.
pub fn voice_key(m: &Member) -> String {
    if m.player.is_empty() {
        format!("guest:{}", m.name.as_str())
    } else {
        m.player.as_str().to_string()
    }
}

/// What the title asks of the party.
#[derive(Clone, Debug)]
pub enum Do {
    Act(u8, u32),
    Invite(String),
}

#[derive(Resource, Default)]
pub struct PartyAsks(pub Vec<Do>);

/// The lobby session, open or on its way, and what it was told.
#[derive(Default)]
struct LobbyRes {
    link: Option<LobbyBox>,
    knock: Option<LobbyKnock>,
    /// When the attempt out was made.
    knocked_at: f32,
    retry_at: f32,
    tries: usize,
    /// The party a link asked for (`?party=`, `--party`), until we're in one.
    want: u32,
    /// The name the session said hello with.
    name: String,
    /// The pass already handed over (`Msg::SignIn`).
    sent_pass: String,
    /// The last microphone state read, and mode.
    mic: Mic,
}

/// The party, from the title on: `want` is a party a link asked for.
pub fn install(app: &mut App, want: u32) {
    let mut view = PartyView::default();
    seed(&mut view);
    app.insert_non_send_resource(LobbyRes { want, ..default() })
        .insert_resource(view)
        .init_resource::<PartyAsks>()
        .add_systems(Update, (pump, lights, talk_hud).chain());
}

/// The session's ticket on the hello if we have one, else a fresh pass,
/// else nothing (a guest).
fn hello(view: &PartyView, door: &Door, name: &str, want: u32) -> Msg {
    let pass = if !view.ticket.is_empty() {
        view.ticket.clone()
    } else {
        door.signin().pass()
    };
    client_net::session::Session::hello(
        name,
        Pass::new(&pass).unwrap_or(Pass::NONE),
        proto::MODE_LOBBY,
        proto::Look::default(),
        want,
    )
}

/// The pass a match's hello carries: the ticket (who the party knows us
/// as), else the sign-in's pass.
pub fn match_pass(view: &PartyView, door: &Door) -> String {
    if view.ticket.is_empty() {
        door.signin().pass()
    } else {
        view.ticket.clone()
    }
}

#[allow(clippy::too_many_arguments)]
fn pump(
    mut lobby: NonSendMut<LobbyRes>,
    door: NonSend<Door>,
    player: Res<Player>,
    mut view: ResMut<PartyView>,
    mut asks: ResMut<PartyAsks>,
    in_match: Res<InMatch>,
    settle: Res<crate::loading::Settle>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed_secs();
    let name = player.sent_name();
    // Not while the first level is still arriving (see FIRST_AFTER_S).
    if lobby.link.is_none() && lobby.knock.is_none() && lobby.tries == 0 && !settle.shown() && now < FIRST_AFTER_S {
        return;
    }
    if lobby.knock.is_some() && now - lobby.knocked_at > KNOCK_GIVE_UP_S {
        warn!("oneshot: the party's line didn't answer; asking again");
        lobby.knock = None;
        lobby.tries += 1;
        lobby.retry_at = now;
    }
    // Open it, or again after it closed, waiting longer each time.
    if lobby.link.as_ref().is_some_and(|l| l.closed()) {
        lobby.link = None;
        view.connected = false;
        view.touch();
    }
    if let Some(k) = lobby.knock.as_mut() {
        if let Some(got) = k.poll() {
            lobby.knock = None;
            match got {
                Ok(l) => {
                    info!("oneshot: the party's line is open");
                    lobby.link = Some(l);
                    lobby.tries = 0;
                    lobby.name = name.clone();
                    lobby.sent_pass = door.signin().pass();
                    view.connected = true;
                    view.touch();
                }
                Err(e) => {
                    warn!("oneshot: the party's line didn't open: {e}");
                    lobby.retry_at = now + RETRY_S[lobby.tries.min(RETRY_S.len() - 1)];
                    lobby.tries += 1;
                }
            }
        }
    }
    if lobby.link.is_none() && lobby.knock.is_none() && now >= lobby.retry_at {
        let want = view.party.as_ref().map_or(lobby.want, |p| p.code);
        lobby.knock = Some(door.lobby(hello(&view, &door, &name, want)));
        lobby.knocked_at = now;
        lobby.retry_at = now + RETRY_S[lobby.tries.min(RETRY_S.len() - 1)];
    }
    // A new name says hello again (the party sees it then).
    if lobby.link.is_some() && lobby.name != name && !name.is_empty() {
        if let Some(l) = lobby.link.as_mut() {
            l.close();
        }
        lobby.link = None;
        lobby.retry_at = now;
    }
    // Its fields apart, so the link and what it was told borrow side by side.
    let lobby = &mut *lobby;
    let Some(link) = lobby.link.as_mut() else {
        asks.0.clear();
        return;
    };
    // Signed in since: the session is that player now.
    let pass = door.signin().pass();
    if !pass.is_empty() && pass != lobby.sent_pass {
        if let Some(p) = Pass::new(&pass) {
            link.send(Msg::SignIn { pass: p });
        }
        lobby.sent_pass = pass;
    }
    for d in asks.0.drain(..) {
        match d {
            Do::Act(act, arg) => link.send(Msg::PartyAct { act, arg }),
            Do::Invite(p) => {
                if let Some(player) = pp::PlayerId::new(&p) {
                    link.send(Msg::PartyInvite { player });
                }
            }
        }
    }
    for m in link.take() {
        match m {
            Msg::Ticket { ticket } => {
                view.ticket = ticket.as_str().to_string();
                keep_ticket(&view.ticket);
            }
            Msg::Party(p) => {
                if view.party.as_ref().is_some_and(|o| o.code != p.code) {
                    view.notice = None;
                }
                view.party = Some(p);
                lobby.want = 0;
                forget_voices(&p);
                view.touch();
            }
            // How the party plays (alone or not) the server already knows.
            Msg::Go { mode, .. } => {
                if !in_match.0 {
                    let party = view.party.as_ref().map_or(0, |p| p.code);
                    let ask = Ask {
                        name: name.clone(),
                        pass: match_pass(&view, &door),
                        mode,
                        look: player.look(),
                        party,
                    };
                    crate::front::PAGE.with(|q| q.borrow_mut().push(crate::front::PageCmd::Knock(ask)));
                }
            }
            Msg::Friends { n, reachable } => {
                view.friends_n = n as usize;
                view.friends_next = vec![None; n as usize];
                view.reachable = reachable;
                if n == 0 {
                    view.friends.clear();
                    view.friends_known = true;
                    view.touch();
                }
            }
            Msg::Friend {
                i,
                name,
                player,
                at,
                party,
                size,
                playing,
            } => {
                if let Some(slot) = view.friends_next.get_mut(i as usize) {
                    *slot = Some(Friend {
                        name: name.as_str().to_string(),
                        player: player.as_str().to_string(),
                        at,
                        party,
                        size,
                        playing,
                    });
                }
                if view.friends_next.iter().all(Option::is_some) && view.friends_next.len() == view.friends_n {
                    let fresh: Vec<Friend> = view.friends_next.iter().flatten().cloned().collect();
                    if fresh != view.friends || !view.friends_known {
                        view.friends = fresh;
                        view.friends_known = true;
                        view.touch();
                    }
                }
            }
            Msg::Invited { from, code } => {
                view.invite = Some((from.as_str().to_string(), code));
                view.touch();
            }
            Msg::Refuse { reason } => {
                if reason == proto::REFUSE_BUSY {
                    // Another tab took over: this one stops asking.
                    lobby.retry_at = f32::INFINITY;
                }
                view.notice = Some(client_net::session::refuse_why(reason).to_string());
                view.touch();
            }
            _ => {}
        }
    }
    // Voice: theirs to the page's speakers, ours from its mic.
    for f in link.take_voice() {
        let tag = f.from & 15;
        let key = view
            .members()
            .iter()
            .find(|m| m.tag == tag)
            .map_or_else(|| format!("tag:{tag}"), voice_key);
        view.heard[tag as usize] = if f.flags & proto::voice::VOICE_END != 0 {
            -1.0
        } else {
            now
        };
        play_voice(&f, &key);
    }
    for f in take_mic() {
        link.talk(&f);
    }
    let mic = mic_state();
    let open = open_mic();
    if mic != lobby.mic || open != view.open_mic {
        lobby.mic = mic;
        view.mic = mic;
        view.open_mic = open;
        view.touch();
    }
}

/// The talk lights on the title's roster: lit while that member talks.
#[derive(Component)]
pub struct TalkLight(pub u8);

fn lights(view: Res<PartyView>, time: Res<Time<Real>>, mut q: Query<(&TalkLight, &mut BackgroundColor)>) {
    let now = time.elapsed_secs();
    for (l, mut bg) in &mut q {
        let c = if view.talking(l.0, now) {
            crate::ui::GOOD
        } else {
            crate::ui::RULE
        };
        bg.set_if_neq(BackgroundColor(c));
    }
}

#[derive(Component)]
struct TalkHud;

/// In a match: who in the party is talking, at the left, out of the way.
fn talk_hud(
    view: Res<PartyView>,
    in_match: Res<InMatch>,
    time: Res<Time<Real>>,
    mut commands: Commands,
    huds: Query<Entity, With<TalkHud>>,
    mut shown: Local<Vec<String>>,
) {
    let now = time.elapsed_secs();
    let want: Vec<String> = if in_match.0 {
        view.members()
            .iter()
            .filter(|m| view.talking(m.tag, now))
            .map(|m| {
                if view.party.as_ref().is_some_and(|p| p.me == m.tag) {
                    "YOU".to_string()
                } else {
                    m.name.as_str().to_uppercase()
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    if *shown == want {
        return;
    }
    shown.clone_from(&want);
    for e in &huds {
        commands.entity(e).despawn();
    }
    if want.is_empty() {
        return;
    }
    commands
        .spawn((
            TalkHud,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(14.0),
                top: Val::Percent(30.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
            GlobalZIndex(4),
        ))
        .with_children(|c| {
            for n in &want {
                c.spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                        border: UiRect::left(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(crate::ui::PANEL_BG),
                    BorderColor::all(crate::ui::GOOD),
                ))
                .with_child((
                    crate::ui::strong(format!("{n} IS TALKING"), 12.0, crate::ui::TITLE),
                    crate::ui::TEXT_SHADOW,
                ));
            }
        });
}

// ── the page's voice (web/voice.js), and the desktop's none ───────────────

#[cfg(target_arch = "wasm32")]
mod page {
    use std::cell::RefCell;

    use js_sys::{Function, Reflect, Uint8Array};
    use proto::voice::VoiceFrame;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    use super::Mic;

    thread_local! {
        static MIC: RefCell<Vec<VoiceFrame>> = const { RefCell::new(Vec::new()) };
    }

    /// The page's microphone, one Opus frame (`web/voice.js`): to the party.
    #[wasm_bindgen]
    pub fn voice_up(seq: u16, end: bool, bytes: &[u8]) -> bool {
        let flags = if end { proto::voice::VOICE_END } else { 0 };
        let Some(f) = VoiceFrame::new(seq, flags, bytes) else {
            return false;
        };
        MIC.with(|q| {
            let mut q = q.borrow_mut();
            if q.len() < 64 {
                q.push(f);
            }
        });
        true
    }

    pub fn take_mic() -> Vec<VoiceFrame> {
        MIC.with(|q| std::mem::take(&mut *q.borrow_mut()))
    }

    fn bridge() -> Option<JsValue> {
        let w = web_sys::window()?;
        Reflect::get(&w, &JsValue::from_str("__oneshotVoice"))
            .ok()
            .filter(|v| v.is_object())
    }

    fn call(name: &str, args: &[JsValue]) -> Option<JsValue> {
        let b = bridge()?;
        let f: Function = Reflect::get(&b, &JsValue::from_str(name)).ok()?.dyn_into().ok()?;
        let a = js_sys::Array::new();
        for x in args {
            a.push(x);
        }
        f.apply(&b, &a).ok()
    }

    pub fn play_voice(f: &VoiceFrame, key: &str) {
        let bytes = Uint8Array::from(f.payload());
        let _ = call(
            "hear",
            &[
                f.from.into(),
                f.seq.into(),
                f.flags.into(),
                bytes.into(),
                JsValue::from_str(key),
            ],
        );
    }

    pub fn mic_state() -> Mic {
        match call("state", &[]).and_then(|v| v.as_string()).as_deref() {
            Some("off") => Mic::Off,
            Some("on") => Mic::On,
            Some("talking") => Mic::Talking,
            _ => Mic::None,
        }
    }

    pub fn open_mic() -> bool {
        call("mode", &[]).and_then(|v| v.as_string()).as_deref() == Some("open")
    }

    pub fn mic_toggle() {
        let _ = call("toggle", &[]);
    }

    pub fn set_open_mic(open: bool) {
        let _ = call("setMode", &[JsValue::from_str(if open { "open" } else { "ptt" })]);
    }

    pub fn muted(key: &str) -> bool {
        call("muted", &[JsValue::from_str(key)])
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn toggle_mute(key: &str) {
        let _ = call("toggleMute", &[JsValue::from_str(key)]);
    }

    pub fn forget_voices(p: &proto::party::Party) {
        let tags = js_sys::Array::new();
        for m in p.members() {
            tags.push(&m.tag.into());
        }
        let _ = call("forget", &[tags.into()]);
    }

    /// The party's link on the clipboard; what was put there.
    pub fn copy_link(code: &str) -> Option<String> {
        let w = web_sys::window()?;
        let f: Function = Reflect::get(&w, &JsValue::from_str("__oneshotCopy"))
            .ok()?
            .dyn_into()
            .ok()?;
        f.call1(&w, &JsValue::from_str(code)).ok()?.as_string()
    }

    /// The ticket kept for this tab: a reload is still who it was.
    pub fn keep_ticket(t: &str) {
        if let Some(s) = web_sys::window().and_then(|w| w.session_storage().ok().flatten()) {
            let _ = s.set_item("oneshot.ticket", t);
        }
    }

    pub fn kept_ticket() -> String {
        web_sys::window()
            .and_then(|w| w.session_storage().ok().flatten())
            .and_then(|s| s.get_item("oneshot.ticket").ok().flatten())
            .unwrap_or_default()
    }

    /// For the browser gates: frames the page played and sent.
    pub fn voice_counts() -> (u32, u32) {
        let n = |k| call(k, &[]).and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        (n("heard"), n("sent"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod page {
    use proto::voice::VoiceFrame;

    use super::Mic;

    pub fn take_mic() -> Vec<VoiceFrame> {
        Vec::new()
    }
    pub fn play_voice(_: &VoiceFrame, _: &str) {}
    pub fn mic_state() -> Mic {
        Mic::None
    }
    pub fn open_mic() -> bool {
        false
    }
    pub fn mic_toggle() {}
    pub fn set_open_mic(_: bool) {}
    pub fn muted(_: &str) -> bool {
        false
    }
    pub fn toggle_mute(_: &str) {}
    pub fn forget_voices(_: &proto::party::Party) {}
    pub fn copy_link(_: &str) -> Option<String> {
        None
    }
    pub fn keep_ticket(_: &str) {}
    pub fn kept_ticket() -> String {
        String::new()
    }
    pub fn voice_counts() -> (u32, u32) {
        (0, 0)
    }
}

pub use page::*;

/// The ticket this tab kept, for the first hello.
pub fn seed(view: &mut PartyView) {
    view.ticket = kept_ticket();
}
