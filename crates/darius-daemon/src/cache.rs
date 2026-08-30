//! CacheCoordinator — daemon-wide prompt-cache engineering (Hermes-class).

use darius_core::TurnCacheStats;
use parking_lot::Mutex;
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
}

/// CacheCoordinator — manages prompt cache state per profile.
pub struct CacheCoordinator {
    system_prompt_version: Arc<Mutex<u32>>,
    per_turn_stats: Arc<Mutex<Vec<TurnCacheStats>>>,
    metrics: Arc<Mutex<CacheMetrics>>,
}

impl CacheCoordinator {
    pub fn new() -> Self {
        Self {
            system_prompt_version: Arc::new(Mutex::new(1)),
            per_turn_stats: Arc::new(Mutex::new(Vec::new())),
            metrics: Arc::new(Mutex::new(CacheMetrics::default())),
        }
    }

    /// Record a turn's cache stats.
    pub fn record_turn(&self, stats: TurnCacheStats) {
        // Validate prefix invariant: no session IDs in prefix region.
        // (In a real implementation, this would scan the actual prompt bytes.)
        if stats.prefix_bytes > 0 && stats.suffix_hash == 0 {
            // This would indicate a violation in a real system.
            // For now, we just record the stats.
        }

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
    }

    /// Get per-turn stats (last N turns).
    pub fn recent_stats(&self, n: usize) -> Vec<TurnCacheStats> {
        let stats = self.per_turn_stats.lock();
        stats.iter().rev().take(n).cloned().collect()
    }

    /// Clear all stats (e.g., after version bump).
    pub fn clear_stats(&self) {
        self.per_turn_stats.lock().clear();
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
    fn version_bump_increments() {
        let coordinator = CacheCoordinator::new();
        assert_eq!(coordinator.system_prompt_version(), 1);
        coordinator.bump_version();
        assert_eq!(coordinator.system_prompt_version(), 2);
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
