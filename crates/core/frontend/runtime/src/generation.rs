use std::collections::{HashMap, HashSet};
use std::sync::Arc;

struct RuntimeGenerationEntry {
    parent: Option<Arc<str>>,
    children: HashSet<Arc<str>>,
    generation: u64,
    subtree_generation: u64,
    runtime: bool,
}

/// Parent/child index for runtime mutation generations.
///
/// Memo validity only needs to know whether anything in an instance's
/// subtree changed. Keeping that aggregate in the index avoids scanning every
/// runtime when a memo entry is stored or probed. Parent placeholders are
/// retained while a child is registered so registration order does not matter;
/// only entries marked as real runtimes are returned to memo lookups.
#[derive(Default)]
pub struct RuntimeGenerationIndex {
    entries: HashMap<Arc<str>, RuntimeGenerationEntry>,
    runtime_count: usize,
    next_subtree_generation: u64,
}

impl RuntimeGenerationIndex {
    fn ensure_entry(&mut self, key: Arc<str>) {
        if self.entries.contains_key(&key) {
            return;
        }
        let parent = parent_instance_key(&key);
        if let Some(parent_key) = parent.as_ref() {
            self.ensure_entry(Arc::clone(parent_key));
            self.entries
                .get_mut(parent_key)
                .expect("parent runtime-generation entry")
                .children
                .insert(Arc::clone(&key));
        }
        self.entries.insert(
            key,
            RuntimeGenerationEntry {
                parent,
                children: HashSet::new(),
                generation: 0,
                subtree_generation: 0,
                runtime: false,
            },
        );
    }

    fn bump_path(&mut self, key: &Arc<str>) {
        self.next_subtree_generation = self.next_subtree_generation.wrapping_add(1);
        if self.next_subtree_generation == 0 {
            self.next_subtree_generation = 1;
        }
        let stamp = self.next_subtree_generation;
        let mut current = Some(Arc::clone(key));
        while let Some(current_key) = current {
            let Some(entry) = self.entries.get_mut(&current_key) else {
                break;
            };
            entry.subtree_generation = stamp;
            current = entry.parent.clone();
        }
    }

    pub fn register(&mut self, key: &str, generation: u64) {
        let key: Arc<str> = Arc::from(key);
        self.ensure_entry(Arc::clone(&key));
        let changed = {
            let entry = self
                .entries
                .get_mut(&key)
                .expect("registered runtime-generation entry");
            if !entry.runtime {
                self.runtime_count += 1;
            }
            let changed = !entry.runtime || entry.generation != generation;
            entry.runtime = true;
            entry.generation = generation;
            changed
        };
        if changed {
            self.bump_path(&key);
        }
    }

    pub fn sync(&mut self, key: &str, generation: u64) {
        let key: Arc<str> = Arc::from(key);
        self.ensure_entry(Arc::clone(&key));
        let changed = {
            let entry = self
                .entries
                .get_mut(&key)
                .expect("synced runtime-generation entry");
            if !entry.runtime {
                self.runtime_count += 1;
            }
            let changed = !entry.runtime || entry.generation != generation;
            entry.runtime = true;
            entry.generation = generation;
            changed
        };
        if changed {
            self.bump_path(&key);
        }
    }

    pub fn subtree_generation(&self, key: &str) -> Option<u64> {
        self.entries
            .get(key)
            .filter(|entry| entry.runtime)
            .map(|entry| entry.subtree_generation)
    }

    /// Monotonic aggregate revision for all runtime generations represented in
    /// this surface. A frontend frame uses this instead of exposing the
    /// private per-instance generation index to shell consumers.
    pub fn revision(&self) -> u64 {
        self.next_subtree_generation
    }

    pub fn runtime_count(&self) -> usize {
        self.runtime_count
    }

    pub fn rebuild(&mut self, runtimes: &[(Arc<str>, u64)]) {
        self.entries.clear();
        self.runtime_count = 0;
        self.next_subtree_generation = 0;
        for (key, generation) in runtimes {
            self.register(key, *generation);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.runtime_count = 0;
        self.next_subtree_generation = 0;
    }
}

fn parent_instance_key(key: &str) -> Option<Arc<str>> {
    key.rsplit_once('/').map(|(parent, _)| Arc::from(parent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_generation_index_aggregates_descendants_without_scanning() {
        let mut index = RuntimeGenerationIndex::default();
        index.register("root", 1);
        let root_generation = index.subtree_generation("root").unwrap();
        index.register("root/local:Child", 1);
        let child_generation = index.subtree_generation("root/local:Child").unwrap();
        let root_after_child = index.subtree_generation("root").unwrap();

        assert_ne!(root_after_child, root_generation);
        assert_ne!(child_generation, 0);

        index.sync("root/local:Child", 2);
        assert_ne!(index.subtree_generation("root").unwrap(), root_after_child);
        assert_eq!(
            index.subtree_generation("root/local:Child"),
            index.subtree_generation("root/local:Child")
        );
    }

    #[test]
    fn runtime_count_excludes_placeholders_and_tracks_rebuilds() {
        let mut index = RuntimeGenerationIndex::default();
        index.register("root/child/grandchild", 1);
        assert_eq!(index.runtime_count(), 1);
        index.sync("root/child/grandchild", 2);
        index.register("root/child/grandchild", 2);
        assert_eq!(index.runtime_count(), 1);
        index.sync("root/child", 1);
        index.register("root", 1);
        assert_eq!(index.runtime_count(), 3);

        index.rebuild(&[(Arc::from("other/child"), 1), (Arc::from("other/child"), 2)]);
        assert_eq!(index.runtime_count(), 1);
        index.clear();
        assert_eq!(index.runtime_count(), 0);
        index.register("other", 1);
        assert_eq!(index.runtime_count(), 1);
    }
}
