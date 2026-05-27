//! O(1) least-recently-used cache.
//!
//! Replaces the `HashMap + VecDeque::retain` pattern used by hot paint/style
//! caches with a doubly-linked free-list arena. All operations (get, insert,
//! evict) are O(1).

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

pub struct LruCache<K, V> {
    capacity: usize,
    map: HashMap<K, usize>,
    nodes: Vec<Option<Node<K, V>>>,
    free: Vec<usize>,
    head: Option<usize>,
    tail: Option<usize>,
}

struct Node<K, V> {
    key: K,
    value: V,
    prev: Option<usize>,
    next: Option<usize>,
}

impl<K, V> std::fmt::Debug for LruCache<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LruCache")
            .field("capacity", &self.capacity)
            .field("len", &self.map.len())
            .finish()
    }
}

impl<K, V> Default for LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    fn default() -> Self {
        Self::new(0)
    }
}

impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::new(),
            nodes: Vec::new(),
            free: Vec::new(),
            head: None,
            tail: None,
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Borrow the entry under `key` and mark it as most-recently-used.
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let idx = *self.map.get(key)?;
        self.move_to_head(idx);
        self.nodes[idx].as_ref().map(|n| &n.value)
    }

    /// Insert (or refresh) an entry. Evicts the least-recent entry when over
    /// capacity. If `capacity == 0`, no eviction happens.
    pub fn insert(&mut self, key: K, value: V) {
        if let Some(&idx) = self.map.get(&key) {
            if let Some(node) = self.nodes[idx].as_mut() {
                node.value = value;
            }
            self.move_to_head(idx);
            return;
        }
        let idx = self.alloc_node(key.clone(), value);
        self.map.insert(key, idx);
        self.push_head(idx);
        if self.capacity > 0 && self.map.len() > self.capacity {
            self.evict_tail();
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.nodes.clear();
        self.free.clear();
        self.head = None;
        self.tail = None;
    }

    /// Remove the entry under `key` and return its value, if any.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let idx = self.map.remove(key)?;
        self.detach(idx);
        let node = self.nodes[idx].take()?;
        self.free.push(idx);
        Some(node.value)
    }

    /// Return true if `key` is present, without updating recency.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.contains_key(key)
    }

    fn alloc_node(&mut self, key: K, value: V) -> usize {
        let node = Node {
            key,
            value,
            prev: None,
            next: None,
        };
        if let Some(idx) = self.free.pop() {
            self.nodes[idx] = Some(node);
            idx
        } else {
            let idx = self.nodes.len();
            self.nodes.push(Some(node));
            idx
        }
    }

    fn push_head(&mut self, idx: usize) {
        let old_head = self.head;
        if let Some(node) = self.nodes[idx].as_mut() {
            node.prev = None;
            node.next = old_head;
        }
        if let Some(h) = old_head {
            if let Some(prev_head) = self.nodes[h].as_mut() {
                prev_head.prev = Some(idx);
            }
        } else {
            self.tail = Some(idx);
        }
        self.head = Some(idx);
    }

    fn detach(&mut self, idx: usize) {
        let (prev, next) = match self.nodes[idx].as_ref() {
            Some(n) => (n.prev, n.next),
            None => return,
        };
        if let Some(p) = prev {
            if let Some(pn) = self.nodes[p].as_mut() {
                pn.next = next;
            }
        } else {
            self.head = next;
        }
        if let Some(nx) = next {
            if let Some(nn) = self.nodes[nx].as_mut() {
                nn.prev = prev;
            }
        } else {
            self.tail = prev;
        }
        if let Some(n) = self.nodes[idx].as_mut() {
            n.prev = None;
            n.next = None;
        }
    }

    fn move_to_head(&mut self, idx: usize) {
        if self.head == Some(idx) {
            return;
        }
        self.detach(idx);
        self.push_head(idx);
    }

    fn evict_tail(&mut self) {
        let Some(tail) = self.tail else {
            return;
        };
        self.detach(tail);
        if let Some(node) = self.nodes[tail].take() {
            self.map.remove(&node.key);
            self.free.push(tail);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used() {
        let mut cache: LruCache<u32, &'static str> = LruCache::new(3);
        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(3, "c");
        // Touch 1 so 2 becomes least recent
        assert_eq!(cache.get(&1), Some(&"a"));
        cache.insert(4, "d");
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&1), Some(&"a"));
        assert_eq!(cache.get(&3), Some(&"c"));
        assert_eq!(cache.get(&4), Some(&"d"));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn insert_refreshes_existing_entry() {
        let mut cache: LruCache<u32, u32> = LruCache::new(2);
        cache.insert(1, 10);
        cache.insert(2, 20);
        cache.insert(1, 11); // refresh, now 2 is oldest
        cache.insert(3, 30);
        assert_eq!(cache.get(&1), Some(&11));
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&3), Some(&30));
    }

    #[test]
    fn clear_resets_state() {
        let mut cache: LruCache<u32, u32> = LruCache::new(2);
        cache.insert(1, 1);
        cache.insert(2, 2);
        cache.clear();
        assert_eq!(cache.len(), 0);
        cache.insert(3, 3);
        assert_eq!(cache.get(&3), Some(&3));
    }

    #[test]
    fn linked_list_consistency_after_many_operations() {
        let mut cache: LruCache<u32, u32> = LruCache::new(4);
        for i in 0..1000 {
            cache.insert(i, i);
            if i % 3 == 0 {
                cache.get(&(i / 2));
            }
        }
        assert!(cache.len() <= 4);
        // Should still be functional
        cache.insert(9999, 9999);
        assert_eq!(cache.get(&9999), Some(&9999));
    }
}
