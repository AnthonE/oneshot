//! The keys, rebindable, after Gates' `client/src/render/binds.rs` (MIT):
//! two binds per action, a primary and an alternate. The mouse's left and
//! right buttons are not bindable (fire, and scope or the left gun), nor is
//! the wheel (swap): a bind may use the middle and side buttons. Left and
//! right modifiers are one key, so CTRL is either Ctrl.

use bevy::prelude::*;

/// One key or one mouse button.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Bind {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// The right-hand twin a left modifier also answers to.
fn twin(k: KeyCode) -> Option<KeyCode> {
    match k {
        KeyCode::ShiftLeft => Some(KeyCode::ShiftRight),
        KeyCode::ControlLeft => Some(KeyCode::ControlRight),
        KeyCode::AltLeft => Some(KeyCode::AltRight),
        _ => None,
    }
}

/// What a capture stores for a pressed key: right modifiers fold into left.
pub fn normalize(k: KeyCode) -> KeyCode {
    match k {
        KeyCode::ShiftRight => KeyCode::ShiftLeft,
        KeyCode::ControlRight => KeyCode::ControlLeft,
        KeyCode::AltRight => KeyCode::AltLeft,
        k => k,
    }
}

/// Keys a player may not bind: Esc (the menu), F9 (the clip, `clip.rs`),
/// and F11 and F12 (fullscreen and the screenshot).
pub const RESERVED: [KeyCode; 4] = [KeyCode::Escape, KeyCode::F9, KeyCode::F11, KeyCode::F12];

/// The mouse buttons a bind may use. Left and right are not among them (see
/// the module header).
pub const MOUSE: [(MouseButton, &str, &str); 3] = [
    (MouseButton::Middle, "Mouse3", "MOUSE 3"),
    (MouseButton::Back, "Mouse4", "MOUSE 4"),
    (MouseButton::Forward, "Mouse5", "MOUSE 5"),
];

/// Every key a bind may use: the key, its token in the settings file, and
/// the label the screen and the prompts show. The token is the `KeyCode`
/// variant's own name, so a hand-edited file reads like the code.
pub const KEYS: [(KeyCode, &str, &str); 93] = [
    (KeyCode::KeyA, "KeyA", "A"),
    (KeyCode::KeyB, "KeyB", "B"),
    (KeyCode::KeyC, "KeyC", "C"),
    (KeyCode::KeyD, "KeyD", "D"),
    (KeyCode::KeyE, "KeyE", "E"),
    (KeyCode::KeyF, "KeyF", "F"),
    (KeyCode::KeyG, "KeyG", "G"),
    (KeyCode::KeyH, "KeyH", "H"),
    (KeyCode::KeyI, "KeyI", "I"),
    (KeyCode::KeyJ, "KeyJ", "J"),
    (KeyCode::KeyK, "KeyK", "K"),
    (KeyCode::KeyL, "KeyL", "L"),
    (KeyCode::KeyM, "KeyM", "M"),
    (KeyCode::KeyN, "KeyN", "N"),
    (KeyCode::KeyO, "KeyO", "O"),
    (KeyCode::KeyP, "KeyP", "P"),
    (KeyCode::KeyQ, "KeyQ", "Q"),
    (KeyCode::KeyR, "KeyR", "R"),
    (KeyCode::KeyS, "KeyS", "S"),
    (KeyCode::KeyT, "KeyT", "T"),
    (KeyCode::KeyU, "KeyU", "U"),
    (KeyCode::KeyV, "KeyV", "V"),
    (KeyCode::KeyW, "KeyW", "W"),
    (KeyCode::KeyX, "KeyX", "X"),
    (KeyCode::KeyY, "KeyY", "Y"),
    (KeyCode::KeyZ, "KeyZ", "Z"),
    (KeyCode::Digit0, "Digit0", "0"),
    (KeyCode::Digit1, "Digit1", "1"),
    (KeyCode::Digit2, "Digit2", "2"),
    (KeyCode::Digit3, "Digit3", "3"),
    (KeyCode::Digit4, "Digit4", "4"),
    (KeyCode::Digit5, "Digit5", "5"),
    (KeyCode::Digit6, "Digit6", "6"),
    (KeyCode::Digit7, "Digit7", "7"),
    (KeyCode::Digit8, "Digit8", "8"),
    (KeyCode::Digit9, "Digit9", "9"),
    (KeyCode::F1, "F1", "F1"),
    (KeyCode::F2, "F2", "F2"),
    (KeyCode::F3, "F3", "F3"),
    (KeyCode::F4, "F4", "F4"),
    (KeyCode::F5, "F5", "F5"),
    (KeyCode::F6, "F6", "F6"),
    (KeyCode::F7, "F7", "F7"),
    (KeyCode::F8, "F8", "F8"),
    (KeyCode::F9, "F9", "F9"),
    (KeyCode::F10, "F10", "F10"),
    (KeyCode::F11, "F11", "F11"),
    (KeyCode::F12, "F12", "F12"),
    (KeyCode::Space, "Space", "SPACE"),
    (KeyCode::Tab, "Tab", "TAB"),
    (KeyCode::Enter, "Enter", "ENTER"),
    (KeyCode::Backspace, "Backspace", "BACKSPACE"),
    (KeyCode::ShiftLeft, "ShiftLeft", "SHIFT"),
    (KeyCode::ControlLeft, "ControlLeft", "CTRL"),
    (KeyCode::AltLeft, "AltLeft", "ALT"),
    (KeyCode::CapsLock, "CapsLock", "CAPS LOCK"),
    (KeyCode::Backquote, "Backquote", "`"),
    (KeyCode::Minus, "Minus", "-"),
    (KeyCode::Equal, "Equal", "="),
    (KeyCode::BracketLeft, "BracketLeft", "L BRACKET"),
    (KeyCode::BracketRight, "BracketRight", "R BRACKET"),
    (KeyCode::Backslash, "Backslash", "\\"),
    (KeyCode::Semicolon, "Semicolon", ";"),
    (KeyCode::Quote, "Quote", "'"),
    (KeyCode::Comma, "Comma", ","),
    (KeyCode::Period, "Period", "."),
    (KeyCode::Slash, "Slash", "/"),
    (KeyCode::ArrowUp, "ArrowUp", "UP"),
    (KeyCode::ArrowDown, "ArrowDown", "DOWN"),
    (KeyCode::ArrowLeft, "ArrowLeft", "LEFT"),
    (KeyCode::ArrowRight, "ArrowRight", "RIGHT"),
    (KeyCode::Insert, "Insert", "INSERT"),
    (KeyCode::Delete, "Delete", "DELETE"),
    (KeyCode::Home, "Home", "HOME"),
    (KeyCode::End, "End", "END"),
    (KeyCode::PageUp, "PageUp", "PAGE UP"),
    (KeyCode::PageDown, "PageDown", "PAGE DOWN"),
    (KeyCode::Numpad0, "Numpad0", "NUM 0"),
    (KeyCode::Numpad1, "Numpad1", "NUM 1"),
    (KeyCode::Numpad2, "Numpad2", "NUM 2"),
    (KeyCode::Numpad3, "Numpad3", "NUM 3"),
    (KeyCode::Numpad4, "Numpad4", "NUM 4"),
    (KeyCode::Numpad5, "Numpad5", "NUM 5"),
    (KeyCode::Numpad6, "Numpad6", "NUM 6"),
    (KeyCode::Numpad7, "Numpad7", "NUM 7"),
    (KeyCode::Numpad8, "Numpad8", "NUM 8"),
    (KeyCode::Numpad9, "Numpad9", "NUM 9"),
    (KeyCode::NumpadAdd, "NumpadAdd", "NUM +"),
    (KeyCode::NumpadSubtract, "NumpadSubtract", "NUM -"),
    (KeyCode::NumpadMultiply, "NumpadMultiply", "NUM *"),
    (KeyCode::NumpadDivide, "NumpadDivide", "NUM /"),
    (KeyCode::NumpadDecimal, "NumpadDecimal", "NUM ."),
    (KeyCode::NumpadEnter, "NumpadEnter", "NUM ENTER"),
];

impl Bind {
    pub fn pressed(self, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
        match self {
            Bind::Key(k) => keys.pressed(k) || twin(k).is_some_and(|t| keys.pressed(t)),
            Bind::Mouse(b) => mouse.pressed(b),
        }
    }

    pub fn just_pressed(self, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
        match self {
            Bind::Key(k) => keys.just_pressed(k) || twin(k).is_some_and(|t| keys.just_pressed(t)),
            Bind::Mouse(b) => mouse.just_pressed(b),
        }
    }

    pub fn just_released(self, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
        match self {
            Bind::Key(k) => keys.just_released(k) || twin(k).is_some_and(|t| keys.just_released(t)),
            Bind::Mouse(b) => mouse.just_released(b),
        }
    }

    /// The settings file's word for this bind, if it is one a bind may use.
    pub fn token(self) -> Option<&'static str> {
        match self {
            Bind::Key(k) => KEYS.iter().find(|e| e.0 == k).map(|e| e.1),
            Bind::Mouse(b) => MOUSE.iter().find(|e| e.0 == b).map(|e| e.1),
        }
    }

    pub fn from_token(t: &str) -> Option<Bind> {
        KEYS.iter()
            .find(|e| e.1 == t)
            .map(|e| Bind::Key(e.0))
            .or_else(|| MOUSE.iter().find(|e| e.1 == t).map(|e| Bind::Mouse(e.0)))
    }

    /// What the screen and the prompts call it.
    pub fn label(self) -> &'static str {
        match self {
            Bind::Key(k) => KEYS.iter().find(|e| e.0 == k).map_or("?", |e| e.2),
            Bind::Mouse(b) => MOUSE.iter().find(|e| e.0 == b).map_or("?", |e| e.2),
        }
    }

    /// Does this key type a character into a text field? Letters, digits,
    /// punctuation and Space do; Tab, Enter and the rest do not.
    pub fn types(self) -> bool {
        match self {
            Bind::Key(KeyCode::Space) => true,
            Bind::Key(_) => self.label().chars().count() == 1,
            Bind::Mouse(_) => false,
        }
    }

    /// May a player bind this at all?
    pub fn bindable(self) -> bool {
        match self {
            Bind::Key(k) => !RESERVED.contains(&k) && self.token().is_some(),
            Bind::Mouse(_) => self.token().is_some(),
        }
    }
}

/// What a key does, in the order the settings list them. Each variant's
/// index is its row in [`SPECS`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Crouch,
    Melee,
    Grenade,
    GrenSwitch,
    Reload,
    Swap,
    Use,
    NetGraph,
    Scores,
}

pub const COUNT: usize = 14;

pub struct Spec {
    pub action: Action,
    /// Its word in the settings file.
    pub token: &'static str,
    /// What the settings screen calls it.
    pub name: &'static str,
    pub default: [Option<Bind>; 2],
}

const fn k(k: KeyCode) -> Option<Bind> {
    Some(Bind::Key(k))
}

/// Every action, its file word, its name and its defaults (the page's
/// keys, after the three.js page's `input.ts`).
pub static SPECS: [Spec; COUNT] = [
    Spec {
        action: Action::Forward,
        token: "forward",
        name: "Forward",
        default: [k(KeyCode::KeyW), k(KeyCode::ArrowUp)],
    },
    Spec {
        action: Action::Back,
        token: "back",
        name: "Back",
        default: [k(KeyCode::KeyS), k(KeyCode::ArrowDown)],
    },
    Spec {
        action: Action::Left,
        token: "left",
        name: "Left",
        default: [k(KeyCode::KeyA), k(KeyCode::ArrowLeft)],
    },
    Spec {
        action: Action::Right,
        token: "right",
        name: "Right",
        default: [k(KeyCode::KeyD), k(KeyCode::ArrowRight)],
    },
    Spec {
        action: Action::Jump,
        token: "jump",
        name: "Jump",
        default: [k(KeyCode::Space), None],
    },
    Spec {
        action: Action::Crouch,
        token: "crouch",
        name: "Crouch",
        default: [k(KeyCode::ControlLeft), k(KeyCode::KeyC)],
    },
    Spec {
        action: Action::Melee,
        token: "melee",
        name: "Melee",
        default: [k(KeyCode::KeyQ), k(KeyCode::KeyV)],
    },
    Spec {
        action: Action::Grenade,
        token: "grenade",
        name: "Throw grenade",
        default: [k(KeyCode::KeyG), None],
    },
    Spec {
        action: Action::GrenSwitch,
        token: "grenade_switch",
        name: "Frag or sticky",
        default: [k(KeyCode::KeyT), None],
    },
    Spec {
        action: Action::Reload,
        token: "reload",
        name: "Reload",
        default: [k(KeyCode::KeyR), None],
    },
    Spec {
        action: Action::Swap,
        token: "swap",
        name: "Swap weapon",
        default: [k(KeyCode::KeyX), k(KeyCode::Digit1)],
    },
    Spec {
        action: Action::Use,
        token: "use",
        name: "Pick up, get in",
        default: [k(KeyCode::KeyE), None],
    },
    Spec {
        action: Action::NetGraph,
        token: "net_graph",
        name: "Net graph",
        default: [k(KeyCode::KeyN), None],
    },
    Spec {
        action: Action::Scores,
        token: "scoreboard",
        name: "Scoreboard (hold)",
        default: [k(KeyCode::Tab), None],
    },
];

impl Action {
    pub fn spec(self) -> &'static Spec {
        &SPECS[self as usize]
    }

    pub fn from_token(t: &str) -> Option<Action> {
        SPECS.iter().find(|s| s.token == t).map(|s| s.action)
    }
}

/// The player's binds: two slots per action, primary and alternate.
#[derive(Resource, Clone, PartialEq, Debug)]
pub struct Binds {
    slots: [[Option<Bind>; 2]; COUNT],
}

impl Default for Binds {
    fn default() -> Self {
        let mut slots = [[None; 2]; COUNT];
        for (i, s) in SPECS.iter().enumerate() {
            slots[i] = s.default;
        }
        Self { slots }
    }
}

impl Binds {
    pub fn slots(&self, a: Action) -> [Option<Bind>; 2] {
        self.slots[a as usize]
    }

    fn each(&self, a: Action) -> impl Iterator<Item = Bind> {
        self.slots[a as usize].into_iter().flatten()
    }

    pub fn pressed(&self, a: Action, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
        self.each(a).any(|b| b.pressed(keys, mouse))
    }

    pub fn just_pressed(&self, a: Action, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
        self.each(a).any(|b| b.just_pressed(keys, mouse))
    }

    /// Put `bind` in `a`'s `slot`. A bind does one thing: if another action
    /// (or `a`'s other slot) had it, it is taken from there, and the action
    /// that lost it is returned so the screen can say so.
    pub fn set(&mut self, a: Action, slot: usize, bind: Option<Bind>) -> Option<Action> {
        let mut lost = None;
        if let Some(b) = bind {
            for (i, slots) in self.slots.iter_mut().enumerate() {
                for (j, s) in slots.iter_mut().enumerate() {
                    if *s == Some(b) && !(i == a as usize && j == slot) {
                        *s = None;
                        if i != a as usize {
                            lost = Some(SPECS[i].action);
                        }
                    }
                }
            }
        }
        self.slots[a as usize][slot] = bind;
        lost
    }

    /// What a prompt calls `a`'s key: its primary, else its alternate.
    pub fn key(&self, a: Action) -> &'static str {
        self.each(a).next().map_or("UNBOUND", Bind::label)
    }

    /// The settings file's `bind.*` lines: only actions that differ from
    /// their defaults, as (action, "primary alt"), `none` for an empty slot.
    pub fn to_lines(&self) -> Vec<(String, String)> {
        SPECS
            .iter()
            .filter(|s| self.slots(s.action) != s.default)
            .map(|s| {
                let words: Vec<&str> = self
                    .slots(s.action)
                    .iter()
                    .map(|b| b.and_then(Bind::token).unwrap_or("none"))
                    .collect();
                (s.token.to_string(), words.join(" "))
            })
            .collect()
    }

    /// Binds from the file's lines over the defaults. An unknown or
    /// reserved key keeps that action's default; lines for actions this
    /// build doesn't know come back as they were, so a save keeps them.
    pub fn from_lines(lines: &[(String, String)]) -> (Self, Vec<(String, String)>) {
        let mut binds = Self::default();
        let mut other = Vec::new();
        for (action, value) in lines {
            let Some(a) = Action::from_token(action) else {
                other.push((action.clone(), value.clone()));
                continue;
            };
            let mut slots = [None; 2];
            let mut ok = true;
            for (i, word) in value.split_whitespace().take(2).enumerate() {
                if word == "none" {
                    continue;
                }
                match Bind::from_token(word).filter(|b| b.bindable()) {
                    Some(b) => slots[i] = Some(b),
                    None => ok = false,
                }
            }
            if ok {
                // Through `set`, so a file naming one key twice can't make
                // it do two things.
                for (i, b) in slots.into_iter().enumerate() {
                    binds.set(a, i, b);
                }
            }
        }
        (binds, other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spec_is_in_its_row() {
        for (i, s) in SPECS.iter().enumerate() {
            assert_eq!(s.action as usize, i, "{}", s.token);
        }
    }

    #[test]
    fn no_key_does_two_things_by_default() {
        let mut seen = Vec::new();
        for s in &SPECS {
            for b in s.default.into_iter().flatten() {
                assert!(b.bindable(), "{} {:?}", s.token, b);
                assert!(!seen.contains(&b), "{:?} twice", b);
                seen.push(b);
            }
        }
    }

    #[test]
    fn every_token_round_trips() {
        for (key, token, _) in KEYS {
            assert_eq!(Bind::from_token(token), Some(Bind::Key(key)));
            assert_eq!(Bind::Key(key).token(), Some(token));
        }
    }

    #[test]
    fn a_rebind_takes_the_key_from_where_it_was() {
        let mut b = Binds::default();
        let lost = b.set(Action::Reload, 0, Some(Bind::Key(KeyCode::KeyE)));
        assert_eq!(lost, Some(Action::Use));
        assert_eq!(b.slots(Action::Use)[0], None);
        assert_eq!(b.key(Action::Reload), "E");
    }

    #[test]
    fn the_file_lines_round_trip_and_defaults_write_nothing() {
        assert!(Binds::default().to_lines().is_empty());
        let mut b = Binds::default();
        b.set(Action::Jump, 1, Some(Bind::Mouse(MouseButton::Back)));
        b.set(Action::Melee, 1, None);
        let lines = b.to_lines();
        let (back, other) = Binds::from_lines(&lines);
        assert_eq!(back, b);
        assert!(other.is_empty());
    }

    #[test]
    fn a_bad_or_reserved_key_keeps_the_default_and_an_unknown_action_is_kept() {
        let lines = vec![
            ("jump".to_string(), "Escape none".to_string()),
            ("reload".to_string(), "NotAKey".to_string()),
            ("dance".to_string(), "KeyB".to_string()),
        ];
        let (b, other) = Binds::from_lines(&lines);
        assert_eq!(b.slots(Action::Jump), Action::Jump.spec().default);
        assert_eq!(b.slots(Action::Reload), Action::Reload.spec().default);
        assert_eq!(other, vec![("dance".to_string(), "KeyB".to_string())]);
    }
}
