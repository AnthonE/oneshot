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
/// Long: a page busy loading its level answers late, and an attempt let go
/// that then lands still holds a seat in the party until its grace is up
/// (the transport's own handshake gives up on a dead server well before).
const KNOCK_GIVE_UP_S: f32 = 60.0;
/// A name being typed says hello again only once it has stood this long
/// (the title's field changes it a key at a time).
const NAME_SETTLE_S: f32 = 1.5;
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
    /// What our wallet owns and its floor, as the server read them (none
    /// till it has said): the armory's cards (`front.rs`).
    pub wardrobe: Option<proto::Wardrobe>,
    /// The last invite: who, and their party's code.
    pub invite: Option<(String, u32)>,
    /// The last thing the party said no to.
    pub notice: Option<String>,
    pub mic: Mic,
    /// Why the mic last didn't open (the page's word: `web/voice.js`
    /// `whyNot`), as the PARTY tab says it; None once it does.
    pub mic_why: Option<&'static str>,
    /// Hold [`PartyView::talk_key`] to talk, or open mic.
    pub open_mic: bool,
    /// What the TALK bind is called (`binds::Action::Talk`).
    pub talk_key: &'static str,
    /// PLAY puts the party alone in a room of its own, bots in the rest.
    pub alone: bool,
    /// That room's own map, score and clock (the leader's to pick).
    pub custom: pp::Custom,
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

/// A member's key for mutes: their Elo Pros id, or for a guest (the wire
/// names no id of theirs) this party's code, their place and their name:
/// never every guest of that name, in every party.
pub fn voice_key(code: u32, m: &Member) -> String {
    if m.player.is_empty() {
        format!("guest:{code:x}:{}:{}", m.tag, m.name.as_str())
    } else {
        m.player.as_str().to_string()
    }
}

/// Each tag's [`voice_key`], made again only when the party changes, not
/// a string a voice frame.
#[derive(Default)]
struct VoiceKeys {
    gen: Option<u32>,
    keys: [String; 16],
}

impl VoiceKeys {
    fn of(&mut self, view: &PartyView, tag: u8) -> &str {
        if self.gen != Some(view.gen) {
            self.gen = Some(view.gen);
            let code = view.party.as_ref().map_or(0, |p| p.code);
            for (t, k) in self.keys.iter_mut().enumerate() {
                *k = view
                    .members()
                    .iter()
                    .find(|m| m.tag as usize == t)
                    .map_or_else(|| format!("tag:{t}"), |m| voice_key(code, m));
            }
        }
        &self.keys[(tag & 15) as usize]
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
    /// The name last seen, and since when it has been that.
    seen_name: String,
    seen_at: f32,
    /// The sign-in's pass already handed over (on a hello or as
    /// `Msg::SignIn`): a pass is good once.
    sent_pass: String,
    /// The sign-in's pass the knock out carries, if it carries one (not the
    /// ticket): handed over once the line opens.
    hello_pass: String,
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
        .add_systems(Update, (voice_prefs, pump, lights, talk_hud).chain());
}

/// The lobby's hello, with [`match_pass`]'s pass.
fn hello(view: &PartyView, door: &Door, name: &str, want: u32) -> Msg {
    let pass = match_pass(view, door);
    client_net::session::Session::hello(
        name,
        Pass::new(&pass).unwrap_or(Pass::NONE),
        proto::MODE_LOBBY,
        proto::Look::default(),
        want,
    )
}

/// A ticket this close to running out is not sent (the server's clock and
/// ours, and a hello on its way), seconds.
const TICKET_MARGIN_S: u64 = 600;

/// What a ticket of the server's says of us (`server/src/auth.rs`
/// `ticket_at`: its prefix, base64url of "id, account, wallet, expiry" a
/// line each, then its mac): signed in or a guest, the wallet, and when it
/// runs out. The server checks it; this only says whether it is still ours
/// to send.
struct TicketSays {
    signed: bool,
    wallet: Option<String>,
    until: u64,
}

/// What every ticket starts with (the wire's, as the server makes them).
use proto::TICKET_PREFIX;

fn ticket_says(t: &str) -> Option<TicketSays> {
    let body = t.strip_prefix(TICKET_PREFIX)?.split_once('.')?.0;
    let text = String::from_utf8(b64url(body)?).ok()?;
    let mut f = text.split('\n');
    let (_id, account, wallet, until) = (f.next()?, f.next()?, f.next()?, f.next()?);
    Some(TicketSays {
        signed: account != "none",
        wallet: (wallet != "none").then(|| wallet.to_string()),
        until: until.parse().ok()?,
    })
}

/// Unpadded base64url, as the server writes a ticket's body.
fn b64url(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Seconds since 1970 on this machine's clock.
fn now_unix() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

/// The pass a hello carries, the lobby's or a match's: the ticket while it
/// still names who we are (not run out, and signed in as the sign-in is),
/// else the sign-in's own pass, else the ticket (a guest stays the same
/// guest), else nothing. Never a guest's ticket while signed in.
pub fn match_pass(view: &PartyView, door: &Door) -> String {
    let signin = door.signin();
    let fresh = signin.pass();
    let (live, ours) = ticket_is(view, &signin);
    if live && ours {
        view.ticket.clone()
    } else if !fresh.is_empty() {
        fresh
    } else if live {
        view.ticket.clone()
    } else {
        String::new()
    }
}

/// Whether the ticket is still good (not about to run out), and whether it
/// is the sign-in's player (or there is no sign-in to be).
fn ticket_is(view: &PartyView, signin: &crate::door::SignIn) -> (bool, bool) {
    let fresh = signin.pass();
    match ticket_says(&view.ticket) {
        Some(t) => (
            t.until > now_unix() + TICKET_MARGIN_S,
            fresh.is_empty()
                || (t.signed
                    && match (t.wallet.as_deref(), signin.wallet()) {
                        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                        (a, b) => a.is_none() && b.is_none(),
                    }),
        ),
        // Not one this build reads: the server still may.
        None => (!view.ticket.is_empty(), fresh.is_empty()),
    }
}

/// How often the page's mic and the sign-in are looked at, seconds: each
/// look is calls into the page and a string made, too many for every frame.
const POLL_S: f32 = 0.1;

/// The name a hello sends (`Player::sent_name`), borrowed, not made.
fn sent_name(player: &Player) -> &str {
    let n = player.name.trim();
    if n.is_empty() {
        "player"
    } else {
        n
    }
}

/// The lobby said no: another tab took the line (this one waits to be
/// looked at again), or the sign-in's pass didn't check out (signed out:
/// the title says so, not "signed in" over a guest).
fn refused(reason: u8, retry_at: &mut f32, view: &mut PartyView, door: &mut Door) {
    if reason == proto::REFUSE_BUSY {
        *retry_at = f32::INFINITY;
    }
    if reason == proto::REFUSE_PASS {
        door.signed_out("Your sign-in ran out: sign in again".into());
    }
    view.notice = Some(client_net::session::refuse_why(reason).to_string());
    view.touch();
}

/// Who the server says we are now. A guest's while the sign-in's pass was
/// handed over: that pass didn't check out (older than its ten minutes).
fn got_ticket(ticket: &str, sent_pass: &str, view: &mut PartyView, door: &mut Door) {
    let fresh = door.signin().pass();
    if !fresh.is_empty() && fresh == sent_pass && ticket_says(ticket).is_some_and(|t| !t.signed) {
        door.signed_out("Your sign-in ran out: sign in again".into());
    }
    view.ticket = ticket.to_string();
    keep_ticket(&view.ticket);
}

#[allow(clippy::too_many_arguments)]
fn pump(
    mut lobby: NonSendMut<LobbyRes>,
    mut door: NonSendMut<Door>,
    player: Res<Player>,
    mut view: ResMut<PartyView>,
    mut asks: ResMut<PartyAsks>,
    in_match: Res<InMatch>,
    settle: Res<crate::loading::Settle>,
    time: Res<Time<Real>>,
    (mut polled, mut mic_frames, mut keys): (Local<f32>, Local<Vec<proto::voice::VoiceFrame>>, Local<VoiceKeys>),
    mut focus: MessageReader<bevy::window::WindowFocused>,
) {
    let now = time.elapsed_secs();
    let name = sent_name(&player);
    // Not while the first level is still arriving (see FIRST_AFTER_S).
    if lobby.link.is_none() && lobby.knock.is_none() && lobby.tries == 0 && !settle.shown() && now < FIRST_AFTER_S {
        return;
    }
    if lobby.knock.is_some() && now - lobby.knocked_at > KNOCK_GIVE_UP_S {
        warn!("oneshot: the party's line didn't answer; asking again");
        lobby.knock = None;
        lobby.tries += 1;
        if lobby.retry_at.is_finite() {
            lobby.retry_at = now;
        }
    }
    // Open it, or again after it closed, waiting longer each time. Its
    // last words first: a refusal says why it closed (another tab took the
    // line, a sign-in that didn't check out).
    if lobby.link.as_ref().is_some_and(|l| l.closed()) {
        if let Some(mut l) = lobby.link.take() {
            for m in l.take() {
                match m {
                    Msg::Refuse { reason } => refused(reason, &mut lobby.retry_at, &mut view, &mut door),
                    Msg::Ticket { ticket } => got_ticket(ticket.as_str(), &lobby.sent_pass, &mut view, &mut door),
                    _ => {}
                }
            }
        }
        view.connected = false;
        view.touch();
    }
    // Back on this tab after another took the line: it asks again.
    if focus.read().any(|f| f.focused) && !lobby.retry_at.is_finite() {
        lobby.retry_at = now;
        lobby.tries = 0;
    }
    if let Some(k) = lobby.knock.as_mut() {
        if let Some(got) = k.poll() {
            lobby.knock = None;
            match got {
                Ok(l) => {
                    info!("oneshot: the party's line is open");
                    lobby.link = Some(l);
                    lobby.tries = 0;
                    lobby.name = name.to_string();
                    // What the hello carried is handed over; a ticket says
                    // nothing of a sign-in since (that goes as SignIn).
                    if !lobby.hello_pass.is_empty() {
                        lobby.sent_pass = std::mem::take(&mut lobby.hello_pass);
                    }
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
        let fresh = door.signin().pass();
        lobby.hello_pass = if !fresh.is_empty() && match_pass(&view, &door) == fresh {
            fresh
        } else {
            String::new()
        };
        lobby.knock = Some(door.lobby(hello(&view, &door, name, want)));
        lobby.knocked_at = now;
        lobby.retry_at = now + RETRY_S[lobby.tries.min(RETRY_S.len() - 1)];
    }
    // A new name says hello again (the party sees it then), once typed.
    if lobby.seen_name != name {
        lobby.seen_name = name.to_string();
        lobby.seen_at = now;
    }
    if lobby.link.is_some() && lobby.name != name && !name.is_empty() && now - lobby.seen_at >= NAME_SETTLE_S {
        if let Some(l) = lobby.link.as_mut() {
            l.close();
        }
        lobby.link = None;
        view.connected = false;
        view.touch();
        // Not a tab that another took the line from.
        if lobby.retry_at.is_finite() {
            lobby.retry_at = now;
        }
    }
    // Its fields apart, so the link and what it was told borrow side by side.
    let lobby = &mut *lobby;
    let Some(link) = lobby.link.as_mut() else {
        // Asked of a line on its way back, they wait for it; of one that
        // stopped (another tab has it), they go.
        if !asks.0.is_empty() && !lobby.retry_at.is_finite() {
            asks.0.clear();
        }
        return;
    };
    let poll = now - *polled >= POLL_S || now < *polled;
    if poll {
        *polled = now;
    }
    // Signed in since: the session is that player now.
    if poll {
        let signin = door.signin();
        let pass = signin.pass();
        if !pass.is_empty() && pass != lobby.sent_pass {
            // The ticket is already that player: nothing to sign in.
            let (live, ours) = ticket_is(&view, &signin);
            if !(live && ours) {
                if let Some(p) = Pass::new(&pass) {
                    link.send(Msg::SignIn { pass: p });
                }
            }
            lobby.sent_pass = pass;
        }
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
            Msg::Ticket { ticket } => got_ticket(ticket.as_str(), &lobby.sent_pass, &mut view, &mut door),
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
                        name: name.to_string(),
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
            Msg::Wardrobe(d) => {
                if view.wardrobe != Some(d) {
                    view.wardrobe = Some(d);
                    view.touch();
                }
            }
            Msg::Refuse { reason } => refused(reason, &mut lobby.retry_at, &mut view, &mut door),
            _ => {}
        }
    }
    // Voice: theirs to the page's speakers, ours from its mic.
    for f in link.take_voice() {
        let tag = f.from & 15;
        let key = keys.of(&view, tag);
        view.heard[tag as usize] = if f.flags & proto::voice::VOICE_END != 0 {
            -1.0
        } else {
            now
        };
        play_voice(&f, key);
    }
    take_mic(&mut mic_frames);
    for f in mic_frames.drain(..) {
        link.talk(&f);
    }
    if !poll {
        return;
    }
    let mic = mic_state();
    let open = open_mic();
    let why = why_words(&mic_why());
    if mic != lobby.mic || open != view.open_mic || why != view.mic_why {
        lobby.mic = mic;
        view.mic = mic;
        view.open_mic = open;
        view.mic_why = why;
        view.touch();
    }
}

/// What the page's voice is told of the settings: the TALK bind's keys
/// and the voice's volume (VOLUME times VOICE). Again when either moves,
/// and until the page has heard it.
fn voice_prefs(
    binds: Res<crate::binds::Binds>,
    settings: Res<crate::settings::Settings>,
    mut view: ResMut<PartyView>,
    mut told: Local<bool>,
) {
    let talk = crate::binds::Action::Talk;
    if view.talk_key != binds.key(talk) {
        view.talk_key = binds.key(talk);
        view.touch();
    }
    if *told && !binds.is_changed() && !settings.is_changed() {
        return;
    }
    *told = set_talk(&binds.tokens(talk)) && set_voice_volume(settings.voice_volume());
}

/// The page's word for why the mic didn't open, as the PARTY tab says it.
fn why_words(why: &str) -> Option<&'static str> {
    Some(match why {
        "" => return None,
        "denied" => "The browser was refused the microphone: allow it for this site, then MIC again.",
        "none" => "No microphone was found.",
        "busy" => "The microphone is in use by something else, or wouldn't start.",
        "unsupported" => "This browser can't do voice.",
        _ => "The microphone didn't open.",
    })
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

/// Who was talking (a bit a tag) in which party (its `gen`), last shown.
type Said = Option<(u16, u32)>;

/// In a match: who in the party is talking, at the left, out of the way.
fn talk_hud(
    view: Res<PartyView>,
    in_match: Res<InMatch>,
    time: Res<Time<Real>>,
    mut commands: Commands,
    huds: Query<Entity, With<TalkHud>>,
    (mut shown, mut said): (Local<Vec<String>>, Local<Said>),
) {
    let now = time.elapsed_secs();
    // Who talks, and the party they're in: the names are made only when
    // that changes, not every frame someone talks.
    let talking = if in_match.0 {
        view.members()
            .iter()
            .filter(|m| view.talking(m.tag, now))
            .fold(0u16, |b, m| b | 1 << (m.tag & 15))
    } else {
        0
    };
    if *said == Some((talking, view.gen)) {
        return;
    }
    *said = Some((talking, view.gen));
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

    /// The page's frames since the last call, swapped into `out` (empty):
    /// both Vecs keep what they hold room for.
    pub fn take_mic(out: &mut Vec<VoiceFrame>) {
        MIC.with(|q| std::mem::swap(&mut *q.borrow_mut(), out));
    }

    /// The page's bridge once it is there, and its functions as asked for.
    type Bridge = (JsValue, Vec<(&'static str, Function)>);

    thread_local! {
        /// Looked up once, not on every call.
        static BRIDGE: RefCell<Option<Bridge>> = const { RefCell::new(None) };
    }

    fn call(name: &'static str, args: &[JsValue]) -> Option<JsValue> {
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            if b.is_none() {
                let w = web_sys::window()?;
                let o = Reflect::get(&w, &JsValue::from_str("__oneshotVoice"))
                    .ok()
                    .filter(|v| v.is_object())?;
                *b = Some((o, Vec::new()));
            }
            let (o, fns) = b.as_mut()?;
            let f = match fns.iter().find(|(n, _)| *n == name) {
                Some((_, f)) => f.clone(),
                None => {
                    let f: Function = Reflect::get(o, &JsValue::from_str(name)).ok()?.dyn_into().ok()?;
                    fns.push((name, f.clone()));
                    f
                }
            };
            match args {
                [] => f.call0(o),
                [x] => f.call1(o, x),
                [x, y] => f.call2(o, x, y),
                [x, y, z] => f.call3(o, x, y, z),
                _ => {
                    let a = js_sys::Array::new();
                    for x in args {
                        a.push(x);
                    }
                    f.apply(o, &a)
                }
            }
            .ok()
        })
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

    /// Why the mic last didn't open (`web/voice.js` `whyNot`), or empty.
    pub fn mic_why() -> String {
        call("why", &[]).and_then(|v| v.as_string()).unwrap_or_default()
    }

    /// The TALK bind's keys, by their `KeyboardEvent.code`s; whether the
    /// page was there to hear it.
    pub fn set_talk(tokens: &str) -> bool {
        call("setTalk", &[JsValue::from_str(tokens)]).is_some()
    }

    pub fn set_voice_volume(v: f32) -> bool {
        call("setVolume", &[v.into()]).is_some()
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
        let n = |k: &'static str| call(k, &[]).and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
        (n("heard"), n("sent"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod page {
    use proto::voice::VoiceFrame;

    use super::Mic;

    pub fn take_mic(_: &mut Vec<VoiceFrame>) {}
    pub fn play_voice(_: &VoiceFrame, _: &str) {}
    pub fn mic_state() -> Mic {
        Mic::None
    }
    pub fn open_mic() -> bool {
        false
    }
    pub fn mic_toggle() {}
    pub fn set_open_mic(_: bool) {}
    pub fn mic_why() -> String {
        String::new()
    }
    pub fn set_talk(_: &str) -> bool {
        true
    }
    pub fn set_voice_volume(_: f32) -> bool {
        true
    }
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

#[cfg(test)]
mod tests {
    use super::ticket_says;

    #[test]
    fn a_ticket_says_who_and_till_when() {
        // As server/src/auth.rs writes them: base64url, unpadded.
        let t = ticket_says("os1.MHhhYmMKYWNjdF8xCjB4YWJjCjE5OTk5OTk5OTk.mac").unwrap();
        assert!(t.signed);
        assert_eq!(t.wallet.as_deref(), Some("0xabc"));
        assert_eq!(t.until, 1_999_999_999);
        let g = ticket_says("os1.Z3Vlc3RfMDBmZgpub25lCm5vbmUKMTI.mac").unwrap();
        assert!(!g.signed && g.wallet.is_none() && g.until == 12);
        assert!(ticket_says("not a ticket").is_none());
    }
}
