mod service_deps;
pub use service_deps::NodeServiceFieldDependencies;

use mesh_core_elements::NodeId;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Deterministic runtime node id derived from the stable runtime key assigned
/// during annotation. This keeps node ids stable across full rebuilds when the
/// logical path is unchanged, which is the minimum identity contract needed for
/// a retained tree/render-object cache.
pub fn stable_runtime_node_id(key: &str) -> NodeId {
    let mut hash = FNV_OFFSET;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // Keep zero out of the generated id space so it remains available as a
    // sentinel for future retained-tree tables.
    if hash == 0 { 1 } else { hash }
}

/// Derive a non-zero child identity from its parent and structural index.
#[inline]
pub fn child_runtime_node_id(parent_id: NodeId, child_index: usize) -> NodeId {
    let mut hash = parent_id ^ 0x9e37_79b9_7f4a_7c15;
    hash ^= (child_index as u64).wrapping_add(1);
    hash = hash.wrapping_mul(FNV_PRIME);
    hash ^= hash >> 32;
    if hash == 0 { 1 } else { hash }
}

/// Reproduce the parent-chain identity assigned by runtime annotation for a
/// structural key such as `root/2/5`. Interaction state stores these readable
/// keys, while retained style/layout indexes use the chained `NodeId`.
pub fn runtime_node_id_for_key(key: &str) -> NodeId {
    let mut segments = key.split('/');
    let root = segments.next().unwrap_or(key);
    let mut node_id = stable_runtime_node_id(root);
    let mut prefix = root.to_owned();
    for segment in segments {
        prefix.push('/');
        prefix.push_str(segment);
        if segment.starts_with("@loop:") {
            node_id = stable_runtime_node_id(&prefix);
        } else if let Ok(child_index) = segment.parse::<usize>() {
            node_id = child_runtime_node_id(node_id, child_index);
        } else {
            // Keep malformed/test-only keys deterministic and non-zero.
            return stable_runtime_node_id(key);
        }
    }
    node_id
}

#[cfg(test)]
mod tests;
