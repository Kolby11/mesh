use crate::attributes::AttributeMap;
use crate::prop::Prop;
use crate::tree::NodeState;

/// The typed pseudo-state vocabulary shared by style matching and runtime
/// state publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PseudoState {
    Hovered,
    Focused,
    Active,
    Disabled,
    ReadOnly,
    Required,
    Selected,
    Checked,
    Expanded,
    Pressed,
    Invalid,
    Value,
    FocusVisible,
    Windowed,
    Fullscreen,
    Maximized,
    Activated,
    Tiled,
}

/// Which runtime boundary owns a pseudo-state's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoStateKind {
    Interaction,
    Attribute,
    Surface,
}

/// One row in the canonical pseudo-state table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PseudoStateSpec {
    pub state: PseudoState,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub bit: u32,
    pub kind: PseudoStateKind,
}

/// The single pseudo-state source of truth for selector names, aliases, and
/// stable dependency bits. Keep additions here table-driven so matching,
/// indexing, invalidation, and diagnostics cannot silently drift apart.
pub static PSEUDO_STATE_TABLE: &[PseudoStateSpec] = &[
    PseudoStateSpec {
        state: PseudoState::Hovered,
        name: "hover",
        aliases: &["hovered"],
        bit: 1 << 0,
        kind: PseudoStateKind::Interaction,
    },
    PseudoStateSpec {
        state: PseudoState::Focused,
        name: "focus",
        aliases: &["focused"],
        bit: 1 << 1,
        kind: PseudoStateKind::Interaction,
    },
    PseudoStateSpec {
        state: PseudoState::Active,
        name: "active",
        aliases: &[],
        bit: 1 << 2,
        kind: PseudoStateKind::Interaction,
    },
    PseudoStateSpec {
        state: PseudoState::Disabled,
        name: "disabled",
        aliases: &[],
        bit: 1 << 3,
        kind: PseudoStateKind::Attribute,
    },
    PseudoStateSpec {
        state: PseudoState::ReadOnly,
        name: "readonly",
        aliases: &["read-only"],
        bit: 1 << 4,
        kind: PseudoStateKind::Attribute,
    },
    PseudoStateSpec {
        state: PseudoState::Required,
        name: "required",
        aliases: &[],
        bit: 1 << 5,
        kind: PseudoStateKind::Attribute,
    },
    PseudoStateSpec {
        state: PseudoState::Selected,
        name: "selected",
        aliases: &[],
        bit: 1 << 6,
        kind: PseudoStateKind::Attribute,
    },
    PseudoStateSpec {
        state: PseudoState::Checked,
        name: "checked",
        aliases: &[],
        bit: 1 << 7,
        kind: PseudoStateKind::Attribute,
    },
    PseudoStateSpec {
        state: PseudoState::Expanded,
        name: "expanded",
        aliases: &[],
        bit: 1 << 8,
        kind: PseudoStateKind::Attribute,
    },
    PseudoStateSpec {
        state: PseudoState::Pressed,
        name: "pressed",
        aliases: &[],
        bit: 1 << 9,
        kind: PseudoStateKind::Attribute,
    },
    PseudoStateSpec {
        state: PseudoState::Invalid,
        name: "invalid",
        aliases: &[],
        bit: 1 << 10,
        kind: PseudoStateKind::Attribute,
    },
    PseudoStateSpec {
        state: PseudoState::Value,
        name: "value",
        aliases: &[],
        bit: 1 << 11,
        kind: PseudoStateKind::Attribute,
    },
    PseudoStateSpec {
        state: PseudoState::FocusVisible,
        name: "focus-visible",
        aliases: &[],
        bit: 1 << 12,
        kind: PseudoStateKind::Interaction,
    },
    PseudoStateSpec {
        state: PseudoState::Fullscreen,
        name: "fullscreen",
        aliases: &[],
        bit: 1 << 13,
        kind: PseudoStateKind::Surface,
    },
    PseudoStateSpec {
        state: PseudoState::Maximized,
        name: "maximized",
        aliases: &[],
        bit: 1 << 14,
        kind: PseudoStateKind::Surface,
    },
    PseudoStateSpec {
        state: PseudoState::Activated,
        name: "activated",
        aliases: &[],
        bit: 1 << 15,
        kind: PseudoStateKind::Surface,
    },
    PseudoStateSpec {
        state: PseudoState::Tiled,
        name: "tiled",
        aliases: &[],
        bit: 1 << 16,
        kind: PseudoStateKind::Surface,
    },
    PseudoStateSpec {
        state: PseudoState::Windowed,
        name: "windowed",
        aliases: &[],
        bit: 1 << 17,
        kind: PseudoStateKind::Surface,
    },
];

impl PseudoState {
    pub fn spec(self) -> &'static PseudoStateSpec {
        PSEUDO_STATE_TABLE
            .iter()
            .find(|spec| spec.state == self)
            .expect("every typed pseudo-state has a table entry")
    }

    pub fn from_name(name: &str) -> Option<Self> {
        PSEUDO_STATE_TABLE
            .iter()
            .find(|spec| spec.name == name || spec.aliases.contains(&name))
            .map(|spec| spec.state)
    }

    /// The node-state flag this pseudo-state reads; equal to its table bit.
    pub fn flag(self) -> NodeState {
        NodeState::from_bits_retain(self.spec().bit)
    }

    pub fn value(self, state: NodeState) -> bool {
        state.intersects(self.flag())
    }

    pub fn set_value(self, state: &mut NodeState, value: bool) {
        state.set(self.flag(), value);
    }

    /// Read an author-facing boolean/value attribute for states whose source
    /// is markup. Interaction and surface states deliberately return `None`.
    pub fn authored_value(self, attributes: &AttributeMap) -> Option<bool> {
        let names: &[Prop] = match self {
            Self::Disabled => &[Prop::Disabled, Prop::AriaDisabled],
            Self::ReadOnly => &[Prop::Readonly, Prop::AriaReadonly],
            Self::Required => &[Prop::Required, Prop::AriaRequired],
            Self::Selected => &[Prop::Selected, Prop::AriaSelected],
            Self::Checked => &[Prop::Checked, Prop::AriaChecked],
            Self::Expanded => &[Prop::Expanded, Prop::Open, Prop::AriaExpanded],
            Self::Pressed => &[Prop::Pressed, Prop::AriaPressed],
            Self::Invalid => &[Prop::Invalid, Prop::AriaInvalid],
            Self::Value => &[Prop::Value, Prop::AriaValuenow],
            Self::Hovered
            | Self::Focused
            | Self::Active
            | Self::FocusVisible
            | Self::Windowed
            | Self::Fullscreen
            | Self::Maximized
            | Self::Activated
            | Self::Tiled => return None,
        };

        names.iter().find_map(|name| {
            attributes.get_value(*name).map(|value| {
                let text = value.to_legacy_string();
                match self {
                    Self::Value => !text.trim().is_empty(),
                    _ => value.legacy_bool() || matches!(text.trim(), "checked" | "disabled"),
                }
            })
        })
    }
}

/// Resolve the state that markup attributes author before live interaction
/// state is applied by the shell.
pub fn authored_element_state(attributes: &AttributeMap) -> NodeState {
    let mut state = NodeState::empty();
    for spec in PSEUDO_STATE_TABLE {
        if let Some(value) = spec.state.authored_value(attributes) {
            spec.state.set_value(&mut state, value);
        }
    }
    state
}

pub fn pseudo_state_specs() -> &'static [PseudoStateSpec] {
    PSEUDO_STATE_TABLE
}

pub fn pseudo_state_mask(state: NodeState) -> u32 {
    (state & NodeState::CSS).bits()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Prop;

    #[test]
    fn table_names_bits_and_accessors_are_exhaustive() {
        assert_eq!(PSEUDO_STATE_TABLE.len(), 18);
        let mut state = NodeState::empty();
        for spec in PSEUDO_STATE_TABLE {
            assert!(NodeState::CSS.contains(spec.state.flag()), "{}", spec.name);
            assert_eq!(PseudoState::from_name(spec.name), Some(spec.state));
            assert_eq!(spec.state.spec(), spec);
            assert!(!spec.state.value(state));
            spec.state.set_value(&mut state, true);
            assert!(spec.state.value(state));
            assert_eq!(pseudo_state_mask(state), spec.bit);
            spec.state.set_value(&mut state, false);
            assert!(!spec.state.value(state));
        }
        assert_eq!(pseudo_state_mask(state), 0);
    }

    #[test]
    fn authored_state_aliases_are_typed_and_boolean() {
        let mut attrs = AttributeMap::default();
        attrs.insert(Prop::AriaReadonly, "true".into());
        attrs.insert(Prop::AriaExpanded, "true".into());
        attrs.insert(Prop::AriaValuenow, "0".into());
        assert_eq!(PseudoState::ReadOnly.authored_value(&attrs), Some(true));
        assert_eq!(PseudoState::Expanded.authored_value(&attrs), Some(true));
        assert_eq!(PseudoState::Value.authored_value(&attrs), Some(true));
    }
}
