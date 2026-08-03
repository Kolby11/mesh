use super::*;

/// Bidirectional index from widget nodes to the service fields they read.
///
/// Built after each full `build_tree()` pass (not on targeted interaction restyle).
/// Answers both directions in O(1).
#[derive(Debug, Default)]
pub(in crate::shell::component) struct NodeServiceFieldDependencies {
    /// node_id → set of (service, field) pairs that node reads
    pub(super) forward: HashMap<NodeId, HashSet<(String, String)>>,
    /// service → field → set of node_ids that read it. The nested shape keeps
    /// reverse lookups borrowed, avoiding two temporary String allocations for
    /// every service update field comparison.
    pub(super) reverse: HashMap<String, HashMap<String, HashSet<NodeId>>>,
}

impl NodeServiceFieldDependencies {
    /// Build the bidirectional index from a fully-annotated WidgetNode tree.
    /// Must be called after `annotate_runtime_tree()` so `node.id` values are stable.
    pub(in crate::shell::component) fn build(root: &WidgetNode) -> Self {
        let mut deps = Self::default();
        collect_node_service_deps(root, &mut deps);
        deps
    }

    /// Returns node IDs that read `(service, field)`. Empty set if none.
    pub(in crate::shell::component) fn nodes_reading_field(
        &self,
        service: &str,
        field: &str,
    ) -> &HashSet<NodeId> {
        static EMPTY: std::sync::OnceLock<HashSet<NodeId>> = std::sync::OnceLock::new();
        self.reverse
            .get(service)
            .and_then(|fields| fields.get(field))
            .unwrap_or_else(|| EMPTY.get_or_init(HashSet::new))
    }

    /// Returns `(service, field)` pairs that `node_id` reads. `None` if not tracked.
    pub(in crate::shell::component) fn fields_read_by_node(
        &self,
        node_id: NodeId,
    ) -> Option<&HashSet<(String, String)>> {
        self.forward.get(&node_id)
    }
}

pub(super) fn collect_node_service_deps(
    node: &WidgetNode,
    deps: &mut NodeServiceFieldDependencies,
) {
    if !node.service_field_reads.is_empty() {
        let entry = deps.forward.entry(node.id).or_default();
        for (service, field) in &node.service_field_reads {
            entry.insert((service.clone(), field.clone()));
            deps.reverse
                .entry(service.clone())
                .or_default()
                .entry(field.clone())
                .or_default()
                .insert(node.id);
        }
    }
    for child in &node.children {
        collect_node_service_deps(child, deps);
    }
}
