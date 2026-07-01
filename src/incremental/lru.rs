use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

pub struct LruMap<K: Clone + Eq + Hash, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    max: usize,
}

impl<K: Clone + Eq + Hash, V> LruMap<K, V> {
    pub fn new(max: usize) -> Self {
        Self {
            map: HashMap::with_capacity(max.min(64)),
            order: VecDeque::with_capacity(max.min(64)),
            max,
        }
    }

    #[cfg(test)]
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            self.promote(key);
            self.map.get(key)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        if self.map.contains_key(key) {
            self.promote(key);
            self.map.get_mut(key)
        } else {
            None
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        if self.max == 0 {
            return;
        }
        if self.map.contains_key(&key) {
            self.map.insert(key.clone(), value);
            self.promote(&key);
            return;
        }

        if self.map.len() >= self.max
            && let Some(oldest) = self.order.pop_front()
        {
            self.map.remove(&oldest);
        }

        self.map.insert(key.clone(), value);
        self.order.push_back(key);
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
        self.map.remove(key)
    }

    #[allow(dead_code)]
    pub fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[allow(dead_code)]
    pub fn max(&self) -> usize {
        self.max
    }

    pub fn evict_one(&mut self) -> Option<K> {
        let key = self.order.pop_front()?;
        self.map.remove(&key);
        Some(key)
    }

    #[cfg(test)]
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter()
    }

    #[allow(dead_code)]
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.map.keys()
    }

    fn promote(&mut self, key: &K) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
            self.order.push_back(key.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_basic_insert_and_get() {
        let mut lru: LruMap<i32, &str> = LruMap::new(3);
        lru.insert(1, "a");
        lru.insert(2, "b");
        assert_eq!(lru.get(&1), Some(&"a"));
        assert_eq!(lru.get(&2), Some(&"b"));
        assert_eq!(lru.len(), 2);
    }

    #[test]
    fn lru_evicts_oldest() {
        let mut lru: LruMap<i32, &str> = LruMap::new(2);
        lru.insert(1, "a");
        lru.insert(2, "b");
        lru.insert(3, "c");
        assert_eq!(lru.len(), 2);
        assert!(lru.get(&1).is_none());
        assert_eq!(lru.get(&2), Some(&"b"));
        assert_eq!(lru.get(&3), Some(&"c"));
    }

    #[test]
    fn lru_access_promotes() {
        let mut lru: LruMap<i32, &str> = LruMap::new(2);
        lru.insert(1, "a");
        lru.insert(2, "b");
        lru.get(&1);
        lru.insert(3, "c");
        assert_eq!(lru.get(&1), Some(&"a"));
        assert!(lru.get(&2).is_none());
        assert_eq!(lru.get(&3), Some(&"c"));
    }

    #[test]
    fn lru_remove() {
        let mut lru: LruMap<i32, &str> = LruMap::new(3);
        lru.insert(1, "a");
        lru.insert(2, "b");
        assert_eq!(lru.remove(&1), Some("a"));
        assert_eq!(lru.len(), 1);
        assert!(lru.get(&1).is_none());
    }

    #[test]
    fn lru_update_existing() {
        let mut lru: LruMap<i32, &str> = LruMap::new(3);
        lru.insert(1, "a");
        lru.insert(1, "b");
        assert_eq!(lru.get(&1), Some(&"b"));
        assert_eq!(lru.len(), 1);
    }

    #[test]
    fn lru_clear() {
        let mut lru: LruMap<i32, &str> = LruMap::new(3);
        lru.insert(1, "a");
        lru.insert(2, "b");
        lru.clear();
        assert_eq!(lru.len(), 0);
        assert!(lru.get(&1).is_none());
    }

    #[test]
    fn lru_iter() {
        let mut lru: LruMap<i32, &str> = LruMap::new(5);
        lru.insert(1, "a");
        lru.insert(2, "b");
        lru.insert(3, "c");
        let pairs: Vec<_> = lru.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(pairs.len(), 3);
        assert!(pairs.contains(&(1, "a")));
    }

    #[test]
    fn lru_max_zero_no_panic() {
        let mut lru: LruMap<i32, &str> = LruMap::new(0);
        lru.insert(1, "a");
        assert_eq!(lru.len(), 0);
    }
}
