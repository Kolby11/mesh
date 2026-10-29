use super::state::*;
use crate::Prop;
use crate::tree::NodeState;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleNodeAttrs<'a> {
    pub(super) tag: &'a str,
    pub(super) classes: ClassList<'a>,
    pub(super) id: Option<&'a str>,
    pub(super) inline_style: Option<&'a str>,
    pub(super) parts: Option<&'a str>,
    pub(super) key: Option<&'a str>,
    pub(super) module_id: Option<&'a str>,
    pub(super) state: NodeState,
    pub(super) state_mask: u32,
    /// The part being styled when these are a control host's attributes
    /// matched for one of its parts (`slider::thumb`).
    pub(super) pseudo_element: Option<&'a str>,
    /// Where a part's style starts instead of the tag's theme defaults.
    pub(super) part_initial: Option<&'a crate::style::ComputedStyle>,
}

/// A control host's matching inputs, owned so its part children can be
/// restyled while the host node is borrowed mutably.
#[derive(Debug, Clone)]
pub(super) struct ControlHostAttrs {
    tag: String,
    classes: Vec<String>,
    id: Option<String>,
    module_id: Option<String>,
    state: NodeState,
}

impl ControlHostAttrs {
    pub(super) fn of(host: &mut crate::tree::WidgetNode) -> Self {
        let attrs = StyleNodeAttrs::from_node(host);
        Self {
            tag: attrs.tag.to_string(),
            classes: attrs.classes.iter().map(str::to_owned).collect(),
            id: attrs.id.map(str::to_owned),
            module_id: attrs.module_id.map(str::to_owned),
            state: attrs.state,
        }
    }

    pub(super) fn for_part<'a>(
        &'a self,
        part: &'a str,
        initial: &'a crate::style::ComputedStyle,
    ) -> StyleNodeAttrs<'a> {
        StyleNodeAttrs {
            module_id: self.module_id.as_deref(),
            pseudo_element: Some(part),
            part_initial: Some(initial),
            ..StyleNodeAttrs::new(&self.tag, &self.classes, self.id.as_deref(), self.state)
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) enum ClassList<'a> {
    #[default]
    Empty,
    Borrowed(&'a [String]),
    Owned(Vec<String>),
}

impl<'a> ClassList<'a> {
    pub(super) fn from_class_slice(classes: &'a [String]) -> Self {
        if classes.is_empty() {
            return Self::Empty;
        }
        if classes
            .iter()
            .any(|class| class.is_empty() || class.chars().any(char::is_whitespace))
        {
            Self::Owned(split_class_values(classes.iter().map(String::as_str)))
        } else {
            Self::Borrowed(classes)
        }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &str> {
        match self {
            Self::Empty => ClassListIter::Empty,
            Self::Borrowed(classes) => ClassListIter::Borrowed(classes.iter()),
            Self::Owned(classes) => ClassListIter::Owned(classes.iter()),
        }
    }

    pub(super) fn has_class(&self, class: &str) -> bool {
        self.iter().any(|candidate| candidate == class)
    }
}

pub(super) enum ClassListIter<'a> {
    Empty,
    Borrowed(std::slice::Iter<'a, String>),
    Owned(std::slice::Iter<'a, String>),
}

impl<'a> Iterator for ClassListIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Borrowed(iter) => iter.next().map(String::as_str),
            Self::Owned(iter) => iter.next().map(String::as_str),
        }
    }
}

pub(super) fn split_class_values<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values
        .flat_map(str::split_whitespace)
        .filter(|class| !class.is_empty())
        .map(str::to_owned)
        .collect()
}

impl<'a> StyleNodeAttrs<'a> {
    pub fn new(tag: &'a str, classes: &'a [String], id: Option<&'a str>, state: NodeState) -> Self {
        Self {
            tag,
            classes: ClassList::from_class_slice(classes),
            id,
            inline_style: None,
            parts: None,
            key: None,
            module_id: None,
            state,
            state_mask: active_state_mask(state),
            pseudo_element: None,
            part_initial: None,
        }
    }

    pub fn from_node(node: &'a mut crate::tree::WidgetNode) -> Self {
        node.refresh_class_tokens_cache();
        let classes = ClassList::from_class_slice(node.class_tokens());
        let authored = node.authored_payload();
        Self {
            tag: authored.tag.as_str(),
            classes,
            id: authored
                .attributes
                .get(Prop::Id)
                .map(|value| value.as_str()),
            inline_style: authored.attributes.get(Prop::Style).map(String::as_str),
            parts: authored.attributes.get(Prop::Part).map(String::as_str),
            key: node.mesh_key(),
            module_id: node.module_id(),
            state: node.state,
            state_mask: active_state_mask(node.state),
            pseudo_element: None,
            part_initial: None,
        }
    }

    pub(super) fn has_class(&self, class: &str) -> bool {
        self.classes.has_class(class)
    }

    pub(super) fn id(&self) -> Option<&str> {
        self.id
    }

    pub(super) fn module_id(&self) -> Option<&str> {
        self.module_id
    }
}
