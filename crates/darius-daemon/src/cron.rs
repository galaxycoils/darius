//! Cron scheduler — time-driven task execution with context chaining.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CronError {
    #[error("cron job not found: {0}")]
    NotFound(String),
    #[error("cron job already exists: {0}")]
    AlreadyExists(String),
    #[error("cron job failed: {0}")]
    Failed(String),
}

/// A cron job schedule.
#[derive(Debug, Clone)]
pub struct CronJob {
    pub id: String,
    pub schedule: String, // cron expression (e.g., "0 * * * *")
    pub command: String,
    pub context_from: Option<String>, // ID of job to chain context from
    pub enabled: bool,
    pub last_run: Option<u64>,
    pub next_run: Option<u64>,
    pub failure_count: u32,
}

/// Cron scheduler.
pub struct CronScheduler {
    jobs: Arc<Mutex<HashMap<String, CronJob>>>,
    max_failures: u32, // circuit breaker threshold
}

impl CronScheduler {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            max_failures: 3,
        }
    }

    /// Set max failures before circuit breaker trips.
    pub fn with_max_failures(mut self, max: u32) -> Self {
        self.max_failures = max;
        self
    }

    /// Add a cron job.
    pub fn add_job(&self, job: CronJob) -> Result<(), CronError> {
        let mut jobs = self.jobs.lock();
        if jobs.contains_key(&job.id) {
            return Err(CronError::AlreadyExists(job.id.clone()));
        }
        jobs.insert(job.id.clone(), job);
        Ok(())
    }

    /// Remove a cron job.
    pub fn remove_job(&self, id: &str) -> Result<(), CronError> {
        let mut jobs = self.jobs.lock();
        jobs.remove(id)
            .ok_or_else(|| CronError::NotFound(id.to_string()))?;
        Ok(())
    }

    /// Get a job by ID.
    pub fn get_job(&self, id: &str) -> Option<CronJob> {
        self.jobs.lock().get(id).cloned()
    }

    /// List all jobs.
    pub fn list_jobs(&self) -> Vec<CronJob> {
        self.jobs.lock().values().cloned().collect()
    }

    /// Enable a job.
    pub fn enable(&self, id: &str) -> Result<(), CronError> {
        let mut jobs = self.jobs.lock();
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| CronError::NotFound(id.to_string()))?;
        job.enabled = true;
        Ok(())
    }

    /// Disable a job.
    pub fn disable(&self, id: &str) -> Result<(), CronError> {
        let mut jobs = self.jobs.lock();
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| CronError::NotFound(id.to_string()))?;
        job.enabled = false;
        Ok(())
    }

    /// Get context from a chained job.
    pub fn get_context_from(&self, id: &str) -> Result<Option<String>, CronError> {
        let jobs = self.jobs.lock();
        let job = jobs
            .get(id)
            .ok_or_else(|| CronError::NotFound(id.to_string()))?;
        Ok(job.context_from.clone())
    }

    /// Record a job run (success or failure).
    pub fn record_run(&self, id: &str, success: bool) -> Result<(), CronError> {
        let mut jobs = self.jobs.lock();
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| CronError::NotFound(id.to_string()))?;
        job.last_run = Some(current_timestamp());
        if success {
            job.failure_count = 0;
        } else {
            job.failure_count += 1;
            // Circuit breaker: disable after max failures.
            if job.failure_count >= self.max_failures {
                job.enabled = false;
            }
        }
        Ok(())
    }

    /// Check if a job is circuit-broken.
    pub fn is_circuit_broken(&self, id: &str) -> bool {
        let jobs = self.jobs.lock();
        jobs.get(id)
            .map(|j| j.failure_count >= self.max_failures)
            .unwrap_or(false)
    }

    /// Get jobs ready to run (stub: returns all enabled jobs).
    pub fn ready_jobs(&self) -> Vec<CronJob> {
        let jobs = self.jobs.lock();
        jobs.values().filter(|j| j.enabled).cloned().collect()
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_get_job() {
        let scheduler = CronScheduler::new();
        let job = CronJob {
            id: "job1".into(),
            schedule: "0 * * * *".into(),
            command: "echo hello".into(),
            context_from: None,
            enabled: true,
            last_run: None,
            next_run: None,
            failure_count: 0,
        };

        scheduler.add_job(job.clone()).unwrap();
        let fetched = scheduler.get_job("job1").unwrap();
        assert_eq!(fetched.id, "job1");
    }

    #[test]
    fn context_chain_a_to_b() {
        let scheduler = CronScheduler::new();

        let job_a = CronJob {
            id: "job_a".into(),
            schedule: "0 * * * *".into(),
            command: "task_a".into(),
            context_from: None,
            enabled: true,
            last_run: None,
            next_run: None,
            failure_count: 0,
        };

        let job_b = CronJob {
            id: "job_b".into(),
            schedule: "0 * * * *".into(),
            command: "task_b".into(),
            context_from: Some("job_a".into()),
            enabled: true,
            last_run: None,
            next_run: None,
            failure_count: 0,
        };

        scheduler.add_job(job_a).unwrap();
        scheduler.add_job(job_b).unwrap();

        let context = scheduler.get_context_from("job_b").unwrap();
        assert_eq!(context, Some("job_a".to_string()));
    }

    #[test]
    fn circuit_breaker_disables_after_max_failures() {
        let scheduler = CronScheduler::new().with_max_failures(3);

        let job = CronJob {
            id: "failing".into(),
            schedule: "0 * * * *".into(),
            command: "fail".into(),
            context_from: None,
            enabled: true,
            last_run: None,
            next_run: None,
            failure_count: 0,
        };

        scheduler.add_job(job).unwrap();

        // Record 3 failures.
        for _ in 0..3 {
            scheduler.record_run("failing", false).unwrap();
        }

        assert!(scheduler.is_circuit_broken("failing"));
        let job = scheduler.get_job("failing").unwrap();
        assert!(!job.enabled);
    }

    #[test]
    fn success_resets_failure_count() {
        let scheduler = CronScheduler::new().with_max_failures(3);

        let job = CronJob {
            id: "recovering".into(),
            schedule: "0 * * * *".into(),
            command: "recover".into(),
            context_from: None,
            enabled: true,
            last_run: None,
            next_run: None,
            failure_count: 0,
        };

        scheduler.add_job(job).unwrap();

        // 2 failures.
        scheduler.record_run("recovering", false).unwrap();
        scheduler.record_run("recovering", false).unwrap();
        assert!(!scheduler.is_circuit_broken("recovering"));

        // Success resets.
        scheduler.record_run("recovering", true).unwrap();
        assert!(!scheduler.is_circuit_broken("recovering"));
    }
}
