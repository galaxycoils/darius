//! CacheCoordinator — daemon-wide prompt-cache engineering (Hermes-class).

use darius_core::TurnCacheStats;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;

/// Cache hit ratio metric.
#[derive(Debug, Clone, Default)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub miss_cost_tokens: u64,
}

impl CacheMetrics {
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn hit_percentage(&self) -> f64 {
        self.hit_ratio() * 100.0
    }
}

/// Compute a deterministic cache key from stable system prefix bytes and tool schemas.
pub fn compute_prefix_cache_key(system_prompt: &str, tool_schemas: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"v1-prompt-prefix:");
    hasher.update(system_prompt.as_bytes());
    hasher.update(b":tools:");
    hasher.update(tool_schemas.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// CacheCoordinator — manages prompt cache state per profile.
pub struct CacheCoordinator {
    system_prompt_version: Arc<Mutex<u32>>,
    per_turn_stats: Arc<Mutex<Vec<TurnCacheStats>>>,
    metrics: Arc<Mutex<CacheMetrics>>,
    cached_keys: Arc<Mutex<HashSet<String>>>,
}

impl CacheCoordinator {
    pub fn new() -> Self {
        Self {
            system_prompt_version: Arc::new(Mutex::new(1)),
            per_turn_stats: Arc::new(Mutex::new(Vec::new())),
            metrics: Arc::new(Mutex::new(CacheMetrics::default())),
            cached_keys: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Check if a cache key is already warm. If warm, records a hit; if new, records a miss and caches it.
    pub fn check_or_record_key(&self, key: &str, estimated_tokens: u64) -> bool {
        let mut keys = self.cached_keys.lock();
        if keys.contains(key) {
            self.record_turn(TurnCacheStats {
                prefix_bytes: key.len(),
                break_offset: 0,
                suffix_hash: 0,
                cache_hit: true,
                miss_cost_tokens: 0,
            });
            true
        } else {
            keys.insert(key.to_string());
            self.record_turn(TurnCacheStats {
                prefix_bytes: key.len(),
                break_offset: 0,
                suffix_hash: 0,
                cache_hit: false,
                miss_cost_tokens: estimated_tokens,
            });
            false
        }
    }

    /// Record a turn's cache stats.
    pub fn record_turn(&self, stats: TurnCacheStats) {
        let mut turn_stats = self.per_turn_stats.lock();
        turn_stats.push(stats.clone());

        let mut metrics = self.metrics.lock();
        if stats.cache_hit {
            metrics.hits += 1;
        } else {
            metrics.misses += 1;
            metrics.miss_cost_tokens += stats.miss_cost_tokens;
        }
    }

    /// Get current metrics.
    pub fn metrics(&self) -> CacheMetrics {
        self.metrics.lock().clone()
    }

    /// Get the current system prompt version.
    pub fn system_prompt_version(&self) -> u32 {
        *self.system_prompt_version.lock()
    }

    /// Bump the system prompt version (invalidates prefix cache).
    pub fn bump_version(&self) {
        let mut version = self.system_prompt_version.lock();
        *version += 1;
        self.cached_keys.lock().clear();
    }

    /// Get per-turn stats (last N turns).
    pub fn recent_stats(&self, n: usize) -> Vec<TurnCacheStats> {
        let stats = self.per_turn_stats.lock();
        stats.iter().rev().take(n).cloned().collect()
    }

    /// Clear all stats (e.g., after version bump).
    pub fn clear_stats(&self) {
        self.per_turn_stats.lock().clear();
        self.cached_keys.lock().clear();
    }
}

impl Default for CacheCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_system_prefix_bytes_produce_same_cache_key() {
        let prompt_a = "You are a helpful coding assistant with strict safety controls.";
        let tools_a = r#"[{"name": "read_file"}, {"name": "write_file"}]"#;

        let key1 = compute_prefix_cache_key(prompt_a, tools_a);
        let key2 = compute_prefix_cache_key(prompt_a, tools_a);
        assert_eq!(key1, key2);

        let tools_b = r#"[{"name": "read_file"}]"#;
        let key3 = compute_prefix_cache_key(prompt_a, tools_b);
        assert_ne!(key1, key3);
    }

    #[test]
    fn cache_coordinator_key_hit_and_miss() {
        let coordinator = CacheCoordinator::new();
        let key = compute_prefix_cache_key("system", "tools");

        // First time: miss
        let hit1 = coordinator.check_or_record_key(&key, 500);
        assert!(!hit1);

        // Second time: hit
        let hit2 = coordinator.check_or_record_key(&key, 500);
        assert!(hit2);

        let metrics = coordinator.metrics();
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.hit_percentage(), 50.0);
    }

    #[test]
    fn cache_coordinator_records_turns() {
        let coordinator = CacheCoordinator::new();

        coordinator.record_turn(TurnCacheStats {
            prefix_bytes: 1000,
            break_offset: 500,
            suffix_hash: 12345,
            cache_hit: true,
            miss_cost_tokens: 0,
        });

        coordinator.record_turn(TurnCacheStats {
            prefix_bytes: 1000,
            break_offset: 500,
            suffix_hash: 12345,
            cache_hit: false,
            miss_cost_tokens: 100,
        });

        let metrics = coordinator.metrics();
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.miss_cost_tokens, 100);
        assert!((metrics.hit_ratio() - 0.5).abs() < 0.01);
    }

    #[test]
    fn version_bump_increments_and_invalidates_cache() {
        let coordinator = CacheCoordinator::new();
        let key = "prefix-key-1";
        assert!(!coordinator.check_or_record_key(key, 100));
        assert!(coordinator.check_or_record_key(key, 100));

        assert_eq!(coordinator.system_prompt_version(), 1);
        coordinator.bump_version();
        assert_eq!(coordinator.system_prompt_version(), 2);

        // Key was invalidated, so next check is a miss again
        assert!(!coordinator.check_or_record_key(key, 100));
    }

    #[test]
    fn recent_stats_returns_last_n() {
        let coordinator = CacheCoordinator::new();

        for i in 0..10 {
            coordinator.record_turn(TurnCacheStats {
                prefix_bytes: 100,
                break_offset: 50,
                suffix_hash: i as u64,
                cache_hit: true,
                miss_cost_tokens: 0,
            });
        }

        let recent = coordinator.recent_stats(3);
        assert_eq!(recent.len(), 3);
        // Most recent first.
        assert_eq!(recent[0].suffix_hash, 9);
    }

    #[test]
    fn clear_stats_resets() {
        let coordinator = CacheCoordinator::new();
        coordinator.record_turn(TurnCacheStats {
            prefix_bytes: 100,
            break_offset: 50,
            suffix_hash: 1,
            cache_hit: true,
            miss_cost_tokens: 0,
        });

        coordinator.clear_stats();
        let recent = coordinator.recent_stats(10);
        assert!(recent.is_empty());
    }
}
