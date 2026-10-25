//! The closed vocabulary of node properties the core reads.
//!
//! Every element-contract attribute, plus the handful of names core passes
//! interpret (`text`, `inert`, the ARIA relations, …), is a [`Prop`]. A prop
//! declares its value type and the [`ChangeClass`] a write to it causes, so a
//! setter can report which frame stages a change reaches without inspecting
//! the name. Names outside this set are free-form attributes (`data-*`,
//! pass-through ARIA) and are treated as [`ChangeClass::FREE_FORM`].

bitflags::bitflags! {
    /// Which frame stages a property change can affect.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    pub struct ChangeClass: u8 {
        /// Selector matching or inline style input changed: restyle.
        const STYLE = 1 << 0;
        /// Measured size or placement changed: relayout.
        const LAYOUT = 1 << 1;
        /// Painted content changed without geometry: repaint.
        const PAINT = 1 << 2;
        /// The semantic projection changed.
        const A11Y = 1 << 3;
        /// Input targeting or focus eligibility changed.
        const HIT = 1 << 4;
        /// Identity, references, disclosure or child surfaces changed.
        const STRUCTURE = 1 << 5;

        /// A free-form attribute can feed attribute selectors and pass-through
        /// accessibility, so its writes restyle and re-project semantics.
        const FREE_FORM = Self::STYLE.bits() | Self::A11Y.bits();
    }
}

/// The value a property's consumers expect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropType {
    Text,
    Number,
    Bool,
    /// One of a closed set of keywords.
    Token,
}

macro_rules! props {
    ($($variant:ident = $name:literal : $ty:ident, $($class:ident)|+;)*) => {
        /// A property name from the closed core vocabulary.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Prop {
            $($variant),*
        }

        impl Prop {
            pub const ALL: &'static [Prop] = &[$(Prop::$variant),*];

            /// The authored attribute name.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Prop::$variant => $name),*
                }
            }

            pub const fn value_type(self) -> PropType {
                match self {
                    $(Prop::$variant => PropType::$ty),*
                }
            }

            /// The stages a write to this property can affect.
            pub const fn change_class(self) -> ChangeClass {
                match self {
                    $(Prop::$variant => ChangeClass::from_bits_retain(
                        0 $(| ChangeClass::$class.bits())+
                    )),*
                }
            }

            /// Resolve an authored name; `None` for free-form attributes. The
            /// `match` lowers to a length switch.
            pub fn from_name(name: &str) -> Option<Prop> {
                Some(match name {
                    $($name => Prop::$variant,)*
                    _ => return None,
                })
            }
        }
    };
}

props! {
    Active = "active": Bool, STYLE | PAINT | A11Y;
    Align = "align": Text, LAYOUT | PAINT;
    Alt = "alt": Text, A11Y;
    Anchor = "anchor": Token, LAYOUT | STRUCTURE;
    AnchorElement = "anchor-element": Text, A11Y | STRUCTURE;
    AnchorRef = "anchor-ref": Text, A11Y | STRUCTURE;
    AnchorTarget = "anchor-target": Text, A11Y | STRUCTURE;
    AriaBusy = "aria-busy": Bool, A11Y;
    AriaChecked = "aria-checked": Token, STYLE | A11Y;
    AriaControls = "aria-controls": Text, A11Y;
    AriaDescribedby = "aria-describedby": Text, A11Y;
    AriaDescription = "aria-description": Text, A11Y;
    AriaDetails = "aria-details": Text, A11Y;
    AriaDisabled = "aria-disabled": Bool, STYLE | A11Y | HIT;
    AriaErrormessage = "aria-errormessage": Text, A11Y;
    AriaExpanded = "aria-expanded": Bool, STYLE | A11Y;
    AriaHaspopup = "aria-haspopup": Token, A11Y;
    AriaHidden = "aria-hidden": Bool, A11Y;
    AriaInvalid = "aria-invalid": Bool, STYLE | A11Y;
    AriaKeyshortcuts = "aria-keyshortcuts": Text, A11Y;
    AriaLabel = "aria-label": Text, A11Y;
    AriaLabelledby = "aria-labelledby": Text, A11Y;
    AriaOwns = "aria-owns": Text, A11Y;
    AriaPressed = "aria-pressed": Token, STYLE | A11Y;
    AriaReadonly = "aria-readonly": Bool, STYLE | A11Y;
    AriaRequired = "aria-required": Bool, STYLE | A11Y;
    AriaRole = "aria-role": Text, A11Y;
    AriaSelected = "aria-selected": Bool, STYLE | A11Y;
    AriaValuemax = "aria-valuemax": Number, A11Y;
    AriaValuemin = "aria-valuemin": Number, A11Y;
    AriaValuenow = "aria-valuenow": Number, STYLE | A11Y;
    Autofocus = "autofocus": Bool, HIT;
    Busy = "busy": Bool, STYLE | A11Y;
    Checked = "checked": Bool, STYLE | PAINT | A11Y;
    Class = "class": Text, STYLE;
    Column = "column": Number, LAYOUT;
    ColumnSpan = "column-span": Number, LAYOUT;
    Columns = "columns": Text, LAYOUT;
    Command = "command": Text, A11Y;
    Component = "component": Text, A11Y | STRUCTURE;
    Content = "content": Text, LAYOUT | PAINT | A11Y;
    DataBenchmark = "data-benchmark": Text, STYLE;
    DataIndex = "data-index": Text, STYLE;
    DataMeshElement = "data-mesh-element": Text, STYLE | A11Y | STRUCTURE;
    DataTooltipDisabled = "data-tooltip-disabled": Bool, STRUCTURE;
    Constrain = "constrain": Token, STRUCTURE;
    Default = "default": Bool, STYLE | A11Y;
    Description = "description": Text, A11Y;
    Destructive = "destructive": Bool, STYLE;
    Disabled = "disabled": Bool, STYLE | PAINT | A11Y | HIT;
    Expanded = "expanded": Bool, STYLE | A11Y;
    FocusScope = "focus-scope": Bool, HIT;
    FontFeatures = "font-features": Text, LAYOUT | PAINT;
    For = "for": Text, A11Y;
    Gap = "gap": Number, LAYOUT;
    Grab = "grab": Bool, STRUCTURE;
    Gravity = "gravity": Token, STRUCTURE;
    Height = "height": Text, LAYOUT;
    Hidden = "hidden": Bool, LAYOUT | PAINT | A11Y | HIT;
    Href = "href": Text, A11Y | HIT;
    Id = "id": Text, STYLE | A11Y | STRUCTURE;
    Indeterminate = "indeterminate": Bool, STYLE | PAINT | A11Y;
    Inert = "inert": Bool, A11Y | HIT;
    Invalid = "invalid": Bool, STYLE | A11Y;
    Justify = "justify": Text, LAYOUT;
    Key = "key": Text, A11Y | STRUCTURE;
    Keybind = "keybind": Text, A11Y | HIT;
    Label = "label": Text, LAYOUT | PAINT | A11Y;
    Lang = "lang": Text, LAYOUT | PAINT | A11Y;
    Layer = "layer": Number, PAINT;
    Masked = "masked": Bool, PAINT;
    Max = "max": Number, PAINT | A11Y;
    MaxHeight = "max-height": Text, LAYOUT;
    MaxWidth = "max-width": Text, LAYOUT;
    Min = "min": Number, PAINT | A11Y;
    MinHeight = "min-height": Text, LAYOUT;
    MinWidth = "min-width": Text, LAYOUT;
    Modal = "modal": Bool, A11Y | HIT;
    Multiline = "multiline": Bool, LAYOUT | PAINT;
    Name = "name": Text, LAYOUT | PAINT | A11Y;
    OffsetX = "offset-x": Number, STRUCTURE;
    OffsetY = "offset-y": Number, STRUCTURE;
    Open = "open": Bool, LAYOUT | PAINT | A11Y | HIT | STRUCTURE;
    OpenOn = "open-on": Token, STRUCTURE;
    Orient = "orient": Token, LAYOUT | PAINT;
    Overflow = "overflow": Text, LAYOUT | PAINT;
    OverflowX = "overflow-x": Text, LAYOUT | PAINT;
    OverflowY = "overflow-y": Text, LAYOUT | PAINT;
    Part = "part": Text, STYLE;
    Placeholder = "placeholder": Text, PAINT | A11Y;
    Pressed = "pressed": Bool, STYLE | A11Y;
    Readonly = "readonly": Bool, STYLE | A11Y;
    Ref = "ref": Text, A11Y | STRUCTURE;
    Required = "required": Bool, STYLE | A11Y;
    Role = "role": Text, A11Y;
    Row = "row": Number, LAYOUT;
    RowSpan = "row-span": Number, LAYOUT;
    Rows = "rows": Text, LAYOUT;
    ScrollX = "scroll-x": Number, PAINT | HIT;
    ScrollY = "scroll-y": Number, PAINT | HIT;
    Selectable = "selectable": Bool, HIT;
    Selected = "selected": Bool, STYLE | PAINT | A11Y;
    Shortcut = "shortcut": Text, A11Y;
    Size = "size": Number, LAYOUT | PAINT;
    Slot = "slot": Text, STRUCTURE;
    Spacing = "spacing": Number, LAYOUT;
    Src = "src": Text, LAYOUT | PAINT;
    State = "state": Token, STYLE;
    Step = "step": Number, PAINT | A11Y;
    Style = "style": Text, STYLE;
    Tabindex = "tabindex": Number, HIT;
    Target = "target": Text, A11Y | STRUCTURE;
    Text = "text": Text, LAYOUT | PAINT | A11Y;
    Title = "title": Text, A11Y;
    Tooltip = "tooltip": Text, A11Y | STRUCTURE;
    TooltipAnchor = "tooltip-anchor": Token, STRUCTURE;
    TooltipFor = "tooltip-for": Text, A11Y | STRUCTURE;
    Type = "type": Text, STYLE | PAINT | A11Y;
    Value = "value": Text, STYLE | LAYOUT | PAINT | A11Y;
    Variant = "variant": Token, STYLE;
    Width = "width": Text, LAYOUT;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{ELEMENT_CONTRACT_DEFS, ElementAttributeType};

    #[test]
    fn names_round_trip_and_every_prop_has_a_change_class() {
        for &prop in Prop::ALL {
            assert_eq!(Prop::from_name(prop.name()), Some(prop));
            assert!(!prop.change_class().is_empty(), "{}", prop.name());
        }
        assert_eq!(Prop::from_name("data-not-in-the-vocabulary"), None);
    }

    /// Binding patches re-derive accessibility only for A11Y-class writes.
    #[test]
    fn props_read_by_accessibility_derivation_are_a11y_class() {
        for source in [
            include_str!("accessibility.rs"),
            include_str!("../../../frontend/compiler/src/render/elements.rs"),
        ] {
            for name in source.split("Prop::").skip(1).map(|rest| {
                rest.split(|ch: char| !ch.is_ascii_alphanumeric())
                    .next()
                    .unwrap()
            }) {
                let prop = Prop::ALL
                    .iter()
                    .copied()
                    .find(|prop| format!("{prop:?}") == name)
                    .unwrap_or_else(|| panic!("unknown Prop::{name}"));
                assert!(
                    prop.change_class().contains(ChangeClass::A11Y),
                    "{} feeds accessibility but is not A11Y-class",
                    prop.name()
                );
            }
        }
    }

    #[test]
    fn every_contract_attribute_is_a_prop_of_its_declared_type() {
        for def in ELEMENT_CONTRACT_DEFS {
            for attribute in def.attributes {
                let prop = Prop::from_name(attribute.name).unwrap_or_else(|| {
                    panic!("contract attribute {} is not a Prop", attribute.name)
                });
                let expected = match attribute.attribute_type {
                    ElementAttributeType::String => PropType::Text,
                    ElementAttributeType::Number => PropType::Number,
                    ElementAttributeType::Boolean => PropType::Bool,
                    ElementAttributeType::Token => PropType::Token,
                };
                assert_eq!(prop.value_type(), expected, "{}", attribute.name);
            }
        }
    }
}
