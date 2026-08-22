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

    /// Mutably borrow the entry under `key` and mark it as most-recently-used.
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let idx = *self.map.get(key)?;
        self.move_to_head(idx);
        self.nodes[idx].as_mut().map(|n| &mut n.value)
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

    /// Remove and return the least-recently-used entry.
    pub fn pop_lru(&mut self) -> Option<(K, V)> {
        let tail = self.tail?;
        self.detach(tail);
        let node = self.nodes[tail].take()?;
        self.map.remove(&node.key);
        self.free.push(tail);
        Some((node.key, node.value))
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
        let _ = self.pop_lru();
    }
}

/// An LRU cache with both entry-count and resident-byte limits.
///
/// The caller supplies the weight of each value because the cache is used for
/// decoded resources whose allocation size is not derivable from `V` alone.
/// An entry larger than the byte budget is rejected rather than temporarily
/// exceeding the limit. Replacing an existing key and evicting old entries
/// update the accounting before the new value becomes visible.
pub struct ByteLruCache<K, V> {
    entries: LruCache<K, (V, usize)>,
    max_entries: usize,
    max_bytes: usize,
    bytes: usize,
}

impl<K, V> std::fmt::Debug for ByteLruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteLruCache")
            .field("max_entries", &self.max_entries)
            .field("max_bytes", &self.max_bytes)
            .field("entries", &self.len())
            .field("bytes", &self.bytes)
            .finish()
    }
}

impl<K, V> ByteLruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: LruCache::new(max_entries),
            max_entries,
            max_bytes,
            bytes: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.entries.get(key).map(|(value, _)| value)
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.entries.contains_key(key)
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let (value, weight) = self.entries.remove(key)?;
        self.bytes = self.bytes.saturating_sub(weight);
        Some(value)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    /// Insert a value, evicting least-recent entries until both budgets hold.
    /// Returns `false` when the value cannot fit or this cache is disabled.
    pub fn insert(&mut self, key: K, value: V, weight: usize) -> bool {
        if self.max_entries == 0 || self.max_bytes == 0 || weight > self.max_bytes {
            let _ = self.remove(&key);
            return false;
        }

        if let Some((_, previous_weight)) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous_weight);
        }

        while self.len() >= self.max_entries || self.bytes.saturating_add(weight) > self.max_bytes {
            let Some((_, (_, evicted_weight))) = self.entries.pop_lru() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(evicted_weight);
        }

        if self.len() >= self.max_entries || self.bytes.saturating_add(weight) > self.max_bytes {
            return false;
        }

        self.entries.insert(key, (value, weight));
        self.bytes = self.bytes.saturating_add(weight);
        true
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
    fn mutable_access_refreshes_existing_entry() {
        let mut cache: LruCache<u32, u32> = LruCache::new(2);
        cache.insert(1, 10);
        cache.insert(2, 20);
        *cache.get_mut(&1).unwrap() = 11;
        cache.insert(3, 30);
        assert_eq!(cache.get(&1), Some(&11));
        assert_eq!(cache.get(&2), None);
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

    #[test]
    fn byte_cache_evicts_oldest_entries_before_exceeding_budget() {
        let mut cache = ByteLruCache::new(3, 10);
        assert!(cache.insert(1, "one", 4));
        assert!(cache.insert(2, "two", 4));
        assert_eq!(cache.get(&1), Some(&"one"));
        assert!(cache.insert(3, "three", 6));

        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&1), Some(&"one"));
        assert_eq!(cache.get(&3), Some(&"three"));
        assert_eq!(cache.bytes(), 10);
    }

    #[test]
    fn byte_cache_rejects_oversized_values_and_accounts_for_replacement() {
        let mut cache = ByteLruCache::new(2, 8);
        assert!(!cache.insert(1, "too large", 9));
        assert!(cache.is_empty());

        assert!(cache.insert(1, "old", 5));
        assert!(cache.insert(1, "new", 3));
        assert_eq!(cache.get(&1), Some(&"new"));
        assert_eq!(cache.bytes(), 3);
        cache.clear();
        assert_eq!(cache.bytes(), 0);
    }
}
