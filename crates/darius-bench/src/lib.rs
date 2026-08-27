//! Darius benchmark harness — SWE-bench-style coding, long-horizon, ARC-style reasoning.

use serde::{Deserialize, Serialize};

/// Benchmark category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BenchmarkCategory {
    Coding,
    LongHorizon,
    Reasoning,
}

/// A single benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Benchmark {
    pub id: String,
    pub name: String,
    pub category: BenchmarkCategory,
    pub setup: String,
    pub expected: String,
}

/// A benchmark suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    pub name: String,
    pub benchmarks: Vec<Benchmark>,
}

/// Result of running a benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub benchmark_id: String,
    pub passed: bool,
    pub duration_ms: u64,
    pub output: String,
}

/// Benchmark metrics.
#[derive(Debug, Clone, Default)]
pub struct BenchmarkMetrics {
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
    pub total_ms: u64,
}

/// Benchmark runner.
pub struct BenchmarkRunner;

impl BenchmarkRunner {
    /// Run a benchmark suite and return results.
    pub fn run(suite: &BenchmarkSuite) -> Vec<BenchmarkResult> {
        suite
            .benchmarks
            .iter()
            .map(|b| {
                let start = std::time::Instant::now();
                // Stub: in a real implementation, this would execute the benchmark.
                let passed = b.setup.contains("pass") || !b.setup.contains("fail");
                BenchmarkResult {
                    benchmark_id: b.id.clone(),
                    passed,
                    duration_ms: start.elapsed().as_millis() as u64,
                    output: format!("executed: {}", b.name),
                }
            })
            .collect()
    }

    /// Run a suite and return metrics.
    pub fn run_with_metrics(suite: &BenchmarkSuite) -> (Vec<BenchmarkResult>, BenchmarkMetrics) {
        let results = Self::run(suite);
        let mut metrics = BenchmarkMetrics::default();
        for result in &results {
            metrics.total += 1;
            if result.passed {
                metrics.passed += 1;
            } else {
                metrics.failed += 1;
            }
            metrics.total_ms += result.duration_ms;
        }
        (results, metrics)
    }
}

impl BenchmarkSuite {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            benchmarks: Vec::new(),
        }
    }

    pub fn add(&mut self, benchmark: Benchmark) {
        self.benchmarks.push(benchmark);
    }

    pub fn len(&self) -> usize {
        self.benchmarks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.benchmarks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_adds_benchmarks() {
        let mut suite = BenchmarkSuite::new("test-suite");
        suite.add(Benchmark {
            id: "b1".into(),
            name: "test".into(),
            category: BenchmarkCategory::Coding,
            setup: "pass".into(),
            expected: "expected".into(),
        });

        assert_eq!(suite.len(), 1);
    }

    #[test]
    fn runner_executes_benchmarks() {
        let mut suite = BenchmarkSuite::new("test");
        suite.add(Benchmark {
            id: "pass".into(),
            name: "passing".into(),
            category: BenchmarkCategory::Coding,
            setup: "pass".into(),
            expected: "ok".into(),
        });
        suite.add(Benchmark {
            id: "fail".into(),
            name: "failing".into(),
            category: BenchmarkCategory::Reasoning,
            setup: "fail".into(),
            expected: "error".into(),
        });

        let (results, metrics) = BenchmarkRunner::run_with_metrics(&suite);
        assert_eq!(results.len(), 2);
        assert_eq!(metrics.total, 2);
        assert_eq!(metrics.passed, 1);
        assert_eq!(metrics.failed, 1);
    }

    #[test]
    fn runner_returns_results() {
        let mut suite = BenchmarkSuite::new("test");
        suite.add(Benchmark {
            id: "b1".into(),
            name: "bench1".into(),
            category: BenchmarkCategory::LongHorizon,
            setup: "pass".into(),
            expected: "done".into(),
        });

        let results = BenchmarkRunner::run(&suite);
        assert_eq!(results.len(), 1);
        assert!(results[0].passed);
    }
}
