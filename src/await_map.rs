use std::{collections::HashMap, hash::Hash};

use parking_lot::RwLock;
use tokio::sync::Notify;

/// `AwaitMap` is a threadsafe hash map that allows readers to block if
/// a key has not been set yet.
#[derive(Default)]
pub struct AwaitMap<K, V>
where
    K: PartialEq + Eq + Hash,
    V: Clone,
{
    notify: Notify,
    inner: RwLock<HashMap<K, V>>,
}

impl<K, V> AwaitMap<K, V>
where
    K: PartialEq + Eq + Hash,
    V: Clone,
{
    pub fn new() -> Self {
        Self {
            notify: Notify::new(),
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Insert the value for `key`. Notify anyone waiting for it
    pub fn insert(&self, key: K, value: V) {
        self.inner.write().insert(key, value);
        self.notify.notify_waiters();
    }

    /// Get the value for `key`. Block if the value has not been set yet
    pub async fn get(&self, key: &K) -> V {
        loop {
            if let Some(value) = self.inner.read().get(key) {
                return value.clone();
            }
            self.notify.notified().await
        }
    }
}
