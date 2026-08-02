use super::state::*;
use crate::tree::ElementState;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleNodeAttrs<'a> {
    pub(super) tag: &'a str,
    pub(super) classes: ClassList<'a>,
    pub(super) id: Option<&'a str>,
    pub(super) inline_style: Option<&'a str>,
    pub(super) key: Option<&'a str>,
    pub(super) module_id: Option<&'a str>,
    pub(super) state: ElementState,
    pub(super) state_mask: u32,
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
    pub fn new(
        tag: &'a str,
        classes: &'a [String],
        id: Option<&'a str>,
        state: ElementState,
    ) -> Self {
        Self {
            tag,
            classes: ClassList::from_class_slice(classes),
            id,
            inline_style: None,
            key: None,
            module_id: None,
            state,
            state_mask: active_state_mask(state),
        }
    }

    pub fn from_node(node: &'a mut crate::tree::WidgetNode) -> Self {
        node.refresh_class_tokens_cache();
        let classes = ClassList::from_class_slice(node.class_tokens());
        let authored = node.authored_payload();
        Self {
            tag: authored.tag.as_str(),
            classes,
            id: authored.attributes.get("id").map(|value| value.as_str()),
            inline_style: authored.attributes.get("style").map(String::as_str),
            key: node.mesh_key(),
            module_id: node.module_id(),
            state: node.state,
            state_mask: active_state_mask(node.state),
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
