//! Darius benchmark harness — SWE-bench-style coding, long-horizon, ARC-style.

pub struct BenchmarkSuite {
    pub name: String,
    pub benchmarks: Vec<Benchmark>,
}

pub struct Benchmark {
    pub id: String,
    pub name: String,
    pub category: BenchmarkCategory,
    pub setup: String,
    pub expected: String,
}

pub enum BenchmarkCategory {
    Coding,
    LongHorizon,
    Reasoning,
}

pub struct BenchmarkRunner;

pub struct BenchmarkMetrics {
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
    pub total_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let suite = BenchmarkSuite {
            name: "test".into(),
            benchmarks: vec![],
        };
        assert_eq!(suite.name, "test");
        let m = BenchmarkMetrics { total: 0, passed: 0, failed: 0, total_ms: 0 };
        assert_eq!(m.total, 0);
    }
}
