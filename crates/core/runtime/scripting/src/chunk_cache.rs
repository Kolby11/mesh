use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

static SOURCE_CACHE: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<u64, String>> {
    SOURCE_CACHE.get_or_init(Default::default)
}

pub struct ChunkCache;

impl ChunkCache {
    pub fn new() -> Self {
        Self
    }

    pub fn get_or_insert(source: &str) -> u64 {
        let key = fnv64(source.as_bytes());
        let mut map = cache().lock().unwrap();
        map.entry(key).or_insert_with(|| source.to_string());
        key
    }

    pub fn get(key: u64) -> Option<String> {
        cache().lock().unwrap().get(&key).cloned()
    }

    pub fn remove(key: u64) -> Option<String> {
        // Phase 95 (CACHE-03): hot-reload mtime watcher calls this to evict on source change.
        cache().lock().unwrap().remove(&key)
    }

    pub fn len() -> usize {
        cache().lock().unwrap().len()
    }
}

impl Default for ChunkCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_or_insert_stores_and_returns_source() {
        let src = "get_or_insert_stores_and_returns_source icon_name = 'a'";
        let key = ChunkCache::get_or_insert(src);
        assert_eq!(key, fnv64(src.as_bytes()));
        assert_eq!(ChunkCache::get(key), Some(src.to_string()));
        ChunkCache::remove(key);
    }

    #[test]
    fn second_lookup_returns_same_string() {
        let src = "second_lookup_returns_same_string local x = 1";
        let len_before = ChunkCache::len();
        let key1 = ChunkCache::get_or_insert(src);
        let key2 = ChunkCache::get_or_insert(src);
        assert_eq!(key1, key2);
        assert_eq!(ChunkCache::len(), len_before + 1);
        ChunkCache::remove(key1);
    }

    #[test]
    fn different_sources_get_different_keys() {
        let src_a = "different_sources_get_different_keys_a";
        let src_b = "different_sources_get_different_keys_b";
        let key_a = ChunkCache::get_or_insert(src_a);
        let key_b = ChunkCache::get_or_insert(src_b);
        assert_ne!(key_a, key_b);
        assert_eq!(ChunkCache::get(key_a), Some(src_a.to_string()));
        assert_eq!(ChunkCache::get(key_b), Some(src_b.to_string()));
        ChunkCache::remove(key_a);
        ChunkCache::remove(key_b);
    }

    #[test]
    fn fnv64_matches_reference_value() {
        assert_eq!(fnv64(b""), FNV_OFFSET);
        let expected_a = (FNV_OFFSET ^ 0x61u64).wrapping_mul(FNV_PRIME);
        assert_eq!(fnv64(b"a"), expected_a);
    }

    #[test]
    fn remove_evicts_entry() {
        let src = "remove_evicts_entry local y = 2";
        let key = ChunkCache::get_or_insert(src);
        assert!(ChunkCache::get(key).is_some());
        ChunkCache::remove(key);
        assert_eq!(ChunkCache::get(key), None);
    }
}
