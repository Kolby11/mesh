//! Pure keybind policy shared by the shell and the settings UI: normalising key
//! names, the keys no binding may claim, and assigning keys to `auto` actions
//! from the user's ordered preference list.

use std::collections::{HashMap, HashSet};

/// Keys that traversal, activation and cancel own. No binding may claim them.
const RESERVED_KEYS: &[&str] = &[
    "tab",
    "enter",
    "return",
    "space",
    "escape",
    "esc",
    "backspace",
    "delete",
    "home",
    "end",
    "pageup",
    "pagedown",
    "arrowup",
    "arrowdown",
    "arrowleft",
    "arrowright",
];

/// The lower-case form every key comparison uses.
pub fn normalize_key(key: &str) -> String {
    key.trim().to_lowercase()
}

pub fn is_reserved_key(key: &str) -> bool {
    RESERVED_KEYS.contains(&normalize_key(key).as_str())
}

/// The user's preference list as single normalised keys, in order, without
/// duplicates and without reserved keys. A string lists characters
/// (`"qwertasd"`); commas or spaces between characters are ignored.
pub fn parse_key_priority(priority: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    priority
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != ',')
        .map(|ch| normalize_key(&ch.to_string()))
        .filter(|key| !is_reserved_key(key) && seen.insert(key.clone()))
        .collect()
}

/// A modifier-qualified key such as `ctrl+h`, as written in settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl KeyChord {
    /// Parses `ctrl+shift+d`, `F1`, `h`. `None` for an empty key.
    pub fn parse(source: &str) -> Option<Self> {
        let mut chord = KeyChord {
            key: String::new(),
            ctrl: false,
            shift: false,
            alt: false,
        };
        for part in source.split('+') {
            match normalize_key(part).as_str() {
                "" => {}
                "ctrl" | "control" => chord.ctrl = true,
                "shift" => chord.shift = true,
                "alt" | "option" => chord.alt = true,
                key => chord.key = key.to_string(),
            }
        }
        (!chord.key.is_empty()).then_some(chord)
    }

    pub fn matches(&self, key: &str, ctrl: bool, shift: bool, alt: bool) -> bool {
        self.key == normalize_key(key)
            && self.ctrl == ctrl
            && self.alt == alt
            && (self.shift == shift || (!self.shift && is_function_key(&self.key)))
    }

    /// Text for a key cap or help row: `Ctrl+H`, `F1`, `F`.
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        parts.push(display_key(&self.key));
        parts.join("+")
    }
}

fn is_function_key(key: &str) -> bool {
    key.strip_prefix('f')
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit()))
}

/// How one key is written on a key cap.
pub fn display_key(key: &str) -> String {
    let key = normalize_key(key);
    if key.chars().count() == 1 || is_function_key(&key) {
        key.to_uppercase()
    } else {
        let mut chars = key.chars();
        chars
            .next()
            .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_default()
    }
}

/// A shell-owned action: active on any MESH surface that holds keyboard focus,
/// resolved before the surface's own bindings, remappable under
/// `shell.keyboard.shell_shortcuts`.
#[derive(Debug, Clone, Copy)]
pub struct ShellAction {
    pub id: &'static str,
    pub default_chord: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const SHELL_ACTIONS: &[ShellAction] = &[
    ShellAction {
        id: "help",
        default_chord: "F1",
        label: "Keyboard shortcuts",
        description: "Open the window listing every keybind and what it does",
    },
    ShellAction {
        id: "hints",
        default_chord: "Ctrl+H",
        label: "Show key hints",
        description: "Show each control's key on the control: hold to peek, tap to keep them on",
    },
    ShellAction {
        id: "debug_overlay",
        default_chord: "Ctrl+Shift+D",
        label: "Debug overlay",
        description: "Toggle the developer overlay",
    },
    ShellAction {
        id: "debug_picker",
        default_chord: "Ctrl+Shift+C",
        label: "Element picker",
        description: "Pick an element to inspect",
    },
];

/// The chord for a shell action: the user's override when it parses, else the
/// default.
pub fn shell_action_chord(
    action: &ShellAction,
    overrides: &HashMap<String, crate::SurfaceShortcutOverride>,
) -> KeyChord {
    overrides
        .get(action.id)
        .and_then(|shortcut| shortcut.key.as_deref())
        .and_then(KeyChord::parse)
        .or_else(|| KeyChord::parse(action.default_chord))
        .expect("shell action defaults parse")
}

/// The shell action bound to this key event, if any.
pub fn shell_action_for_key(
    overrides: &HashMap<String, crate::SurfaceShortcutOverride>,
    key: &str,
    ctrl: bool,
    shift: bool,
    alt: bool,
) -> Option<&'static ShellAction> {
    SHELL_ACTIONS
        .iter()
        .find(|action| shell_action_chord(action, overrides).matches(key, ctrl, shift, alt))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutoAssignment {
    /// Action id to the bare key it received.
    pub keys: HashMap<String, String>,
    /// Actions that found no free key, in input order.
    pub unassigned: Vec<String>,
}

/// Assigns a bare key to each auto action.
///
/// `actions` is in the stable order the caller derives (module, then
/// declaration). `taken` holds the normalised bare keys that explicit bindings
/// and overrides already use. A key in `cached` is kept while it is still in
/// the priority list and free, so adding an action never moves existing keys;
/// every other action takes the first free key in `priority` order.
pub fn assign_auto_keys(
    actions: &[String],
    taken: &HashSet<String>,
    priority: &[String],
    cached: &HashMap<String, String>,
) -> AutoAssignment {
    let mut used: HashSet<String> = taken.clone();
    let mut assignment = AutoAssignment::default();

    for action in actions {
        let kept = cached
            .get(action)
            .map(|key| normalize_key(key))
            .filter(|key| priority.contains(key) && !used.contains(key));
        if let Some(key) = kept {
            used.insert(key.clone());
            assignment.keys.insert(action.clone(), key);
        }
    }
    for action in actions {
        if assignment.keys.contains_key(action) {
            continue;
        }
        match priority.iter().find(|key| !used.contains(*key)) {
            Some(key) => {
                used.insert(key.clone());
                assignment.keys.insert(action.clone(), key.clone());
            }
            None => assignment.unassigned.push(action.clone()),
        }
    }
    assignment
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn priority_drops_duplicates_reserved_keys_and_separators() {
        assert_eq!(
            parse_key_priority("q w, e q\t"),
            ids(&["q", "w", "e"])
        );
        assert!(parse_key_priority(" ").is_empty());
    }

    #[test]
    fn actions_take_priority_keys_in_order() {
        let assignment = assign_auto_keys(
            &ids(&["open", "find", "close"]),
            &HashSet::new(),
            &parse_key_priority("qwertasd"),
            &HashMap::new(),
        );
        assert_eq!(assignment.keys["open"], "q");
        assert_eq!(assignment.keys["find"], "w");
        assert_eq!(assignment.keys["close"], "e");
        assert!(assignment.unassigned.is_empty());
    }

    #[test]
    fn taken_keys_are_skipped() {
        let taken = HashSet::from(["q".to_string(), "e".to_string()]);
        let assignment = assign_auto_keys(
            &ids(&["a", "b"]),
            &taken,
            &parse_key_priority("qwert"),
            &HashMap::new(),
        );
        assert_eq!(assignment.keys["a"], "w");
        assert_eq!(assignment.keys["b"], "r");
    }

    #[test]
    fn cached_keys_survive_a_new_action_that_sorts_earlier() {
        let priority = parse_key_priority("qwert");
        let first = assign_auto_keys(&ids(&["b", "c"]), &HashSet::new(), &priority, &HashMap::new());
        let second = assign_auto_keys(&ids(&["a", "b", "c"]), &HashSet::new(), &priority, &first.keys);
        assert_eq!(second.keys["b"], first.keys["b"]);
        assert_eq!(second.keys["c"], first.keys["c"]);
        assert_eq!(second.keys["a"], "e", "the new action takes the next free key");
    }

    #[test]
    fn a_cached_key_that_is_now_taken_or_unlisted_is_reassigned() {
        let priority = parse_key_priority("qw");
        let cached = HashMap::from([("a".to_string(), "q".to_string()), ("b".to_string(), "z".to_string())]);
        let taken = HashSet::from(["q".to_string()]);
        let assignment = assign_auto_keys(&ids(&["a", "b"]), &taken, &priority, &cached);
        assert_eq!(assignment.keys["a"], "w");
        assert_eq!(assignment.unassigned, ids(&["b"]));
    }

    #[test]
    fn allocation_is_deterministic() {
        let priority = parse_key_priority("asdf");
        let actions = ids(&["x", "y", "z"]);
        let one = assign_auto_keys(&actions, &HashSet::new(), &priority, &HashMap::new());
        let two = assign_auto_keys(&actions, &HashSet::new(), &priority, &HashMap::new());
        assert_eq!(one, two);
    }

    #[test]
    fn shell_actions_resolve_defaults_and_overrides() {
        let none = HashMap::new();
        assert_eq!(shell_action_for_key(&none, "F1", false, false, false).unwrap().id, "help");
        assert_eq!(shell_action_for_key(&none, "h", true, false, false).unwrap().id, "hints");
        assert!(shell_action_for_key(&none, "h", false, false, false).is_none());
        let overrides = HashMap::from([(
            "help".to_string(),
            crate::SurfaceShortcutOverride { key: Some("ctrl+/".into()) },
        )]);
        assert_eq!(shell_action_for_key(&overrides, "/", true, false, false).unwrap().id, "help");
        assert!(shell_action_for_key(&overrides, "F1", false, false, false).is_none());
    }

    #[test]
    fn chords_parse_match_and_display() {
        let chord = KeyChord::parse("Ctrl+H").unwrap();
        assert!(chord.matches("h", true, false, false));
        assert!(chord.matches("H", true, false, false));
        assert!(!chord.matches("h", false, false, false));
        assert_eq!(chord.display(), "Ctrl+H");
        let f1 = KeyChord::parse("F1").unwrap();
        assert!(f1.matches("F1", false, false, false));
        assert_eq!(f1.display(), "F1");
        assert!(KeyChord::parse("ctrl+").is_none());
    }
}
