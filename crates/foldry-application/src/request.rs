use std::{collections::HashMap, hash::Hash};

use foldry_core::CancellationToken;

#[derive(Clone, Debug)]
pub struct LatestRequest {
    pub generation: u64,
    pub cancellation: CancellationToken,
}

/// Cancels the previous worker request for the same browser or preview key.
pub struct LatestRequestRegistry<K> {
    next_generation: u64,
    active: HashMap<K, LatestRequest>,
}

impl<K> Default for LatestRequestRegistry<K> {
    fn default() -> Self {
        Self {
            next_generation: 0,
            active: HashMap::new(),
        }
    }
}

impl<K> LatestRequestRegistry<K>
where
    K: Eq + Hash,
{
    pub fn begin(&mut self, key: K) -> LatestRequest {
        if let Some(previous) = self.active.remove(&key) {
            previous.cancellation.cancel();
        }
        self.next_generation = self.next_generation.saturating_add(1);
        let request = LatestRequest {
            generation: self.next_generation,
            cancellation: CancellationToken::default(),
        };
        self.active.insert(key, request.clone());
        request
    }

    pub fn cancel(&mut self, key: &K) -> bool {
        self.active.remove(key).is_some_and(|request| {
            request.cancellation.cancel();
            true
        })
    }

    #[must_use]
    pub fn is_current(&self, key: &K, generation: u64) -> bool {
        self.active
            .get(key)
            .is_some_and(|request| request.generation == generation)
    }

    pub fn finish(&mut self, key: &K, generation: u64) -> bool {
        if self.is_current(key, generation) {
            self.active.remove(key);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LatestRequestRegistry;

    #[test]
    fn newer_request_cancels_and_supersedes_the_previous_generation() {
        let mut registry = LatestRequestRegistry::default();
        let first = registry.begin("directory");
        let second = registry.begin("directory");

        assert!(first.cancellation.is_cancelled());
        assert!(!second.cancellation.is_cancelled());
        assert!(!registry.is_current(&"directory", first.generation));
        assert!(registry.is_current(&"directory", second.generation));
        assert!(!registry.finish(&"directory", first.generation));
        assert!(registry.finish(&"directory", second.generation));
    }
}
