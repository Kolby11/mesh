use std::collections::HashMap;

use crate::{ChangeClass, NodeId};

/// Coalesced changes to one retained frame. Consumers may add downstream
/// changes, such as geometry moved by layout, before the frame is published.
#[derive(Debug, Default)]
pub struct FrameDirty {
    nodes: HashMap<NodeId, ChangeClass>,
}

impl FrameDirty {
    /// Merge a write's change classes; unchanged writes do not enqueue a node.
    pub fn mark(&mut self, id: NodeId, change: ChangeClass) {
        if !change.is_empty() {
            *self.nodes.entry(id).or_default() |= change;
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (NodeId, ChangeClass)> + '_ {
        self.nodes.iter().map(|(&id, &change)| (id, change))
    }

    pub fn contains(&self, class: ChangeClass) -> bool {
        self.nodes.values().any(|change| change.intersects(class))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AttributeMap, Prop};

    #[test]
    fn writes_coalesce_and_unchanged_values_do_not_enqueue() {
        let mut attributes = AttributeMap::new();
        let mut dirty = FrameDirty::default();
        dirty.mark(1, attributes.set(Prop::Content, "label".into()));
        dirty.mark(1, ChangeClass::HIT);
        dirty.mark(2, attributes.set(Prop::Content, "label".into()));
        let entries: Vec<_> = dirty.iter().collect();
        assert_eq!(
            entries,
            vec![(1, Prop::Content.change_class() | ChangeClass::HIT)]
        );
        dirty.clear();
        assert_eq!(dirty.iter().count(), 0);
    }
}
