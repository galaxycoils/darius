//! Cron scheduler — time-driven task execution with memory continuity and notepad.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
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
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("memory error: {0}")]
    Memory(#[from] darius_memory::MemoryError),
}

/// A cron job schedule with memory continuity and persistent notepad.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub schedule: String, // cron expression (e.g., "0 * * * *")
    pub command: String,
    pub context_from: Option<String>, // ID of job to chain context from
    pub enabled: bool,
    pub continuity: bool,                 // Learn and remember between runs
    pub notepad: String,                  // Persistent memory / notes across runs
    pub last_output_hash: Option<String>, // Hash of monitor payload for change detection
    pub last_run: Option<u64>,
    pub next_run: Option<u64>,
    pub failure_count: u32,
}

impl CronJob {
    pub fn new(
        id: impl Into<String>,
        schedule: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            schedule: schedule.into(),
            command: command.into(),
            context_from: None,
            enabled: true,
            continuity: true,
            notepad: String::new(),
            last_output_hash: None,
            last_run: None,
            next_run: None,
            failure_count: 0,
        }
    }
}

/// Cron scheduler managing jobs, continuity, and execution triggers.
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
        let mut list: Vec<CronJob> = self.jobs.lock().values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
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

    /// Append a note entry to the job's persistent notepad.
    pub fn append_notepad(&self, id: &str, note: &str) -> Result<(), CronError> {
        let mut jobs = self.jobs.lock();
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| CronError::NotFound(id.to_string()))?;
        if !job.notepad.is_empty() {
            job.notepad.push_str("\n---\n");
        }
        job.notepad.push_str(note.trim());
        Ok(())
    }

    /// Replace the job's notepad content.
    pub fn set_notepad(&self, id: &str, notepad: &str) -> Result<(), CronError> {
        let mut jobs = self.jobs.lock();
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| CronError::NotFound(id.to_string()))?;
        job.notepad = notepad.to_string();
        Ok(())
    }

    /// Check if a monitor payload has changed by comparing hashes.
    /// Returns Ok(true) if payload changed (or was empty), Ok(false) if identical (skip model call).
    pub fn check_and_update_hash(&self, id: &str, payload: &str) -> Result<bool, CronError> {
        let hash = blake3::hash(payload.as_bytes()).to_hex().to_string();
        let mut jobs = self.jobs.lock();
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| CronError::NotFound(id.to_string()))?;

        if let Some(ref prior) = job.last_output_hash
            && prior == &hash
        {
            return Ok(false); // Unchanged -> skip LLM
        }

        job.last_output_hash = Some(hash);
        Ok(true)
    }

    /// Build context for a cron job invocation, including chained context, notepad, and memory pack.
    pub fn build_job_context(
        &self,
        id: &str,
        memory: Option<&darius_memory::MemoryEngine>,
    ) -> Result<String, CronError> {
        let job = self
            .get_job(id)
            .ok_or_else(|| CronError::NotFound(id.to_string()))?;

        let mut context = format!(
            "=== Cron Job: {} ===\nSchedule: {}\nCommand: {}\n",
            job.id, job.schedule, job.command
        );

        if let Some(ref chained_id) = job.context_from
            && let Some(chained_job) = self.get_job(chained_id)
        {
            context.push_str(&format!(
                "\n[Chained Context from {}]:\n{}\n",
                chained_id, chained_job.notepad
            ));
        }

        if job.continuity && !job.notepad.is_empty() {
            context.push_str(&format!(
                "\n[Notepad / Prior Continuity]:\n{}\n",
                job.notepad
            ));
        }

        if let Some(mem) = memory
            && let Ok(pack) = mem.build_pack(3500, 12)
            && !pack.plain.is_empty()
        {
            context.push_str(&format!("\n[Durable Memory]:\n{}\n", pack.plain));
        }

        Ok(context)
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

    /// Get jobs ready to run (returns all enabled jobs).
    pub fn ready_jobs(&self) -> Vec<CronJob> {
        let jobs = self.jobs.lock();
        jobs.values().filter(|j| j.enabled).cloned().collect()
    }

    /// Persist jobs to `cron.json` in directory.
    pub fn save_to_dir(&self, dir: &Path) -> Result<(), CronError> {
        let jobs = self.list_jobs();
        let path = dir.join("cron.json");
        let json = serde_json::to_string_pretty(&jobs)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load jobs from `cron.json` in directory.
    pub fn load_from_dir(&self, dir: &Path) -> Result<(), CronError> {
        let path = dir.join("cron.json");
        if !path.exists() {
            return Ok(());
        }
        let data = std::fs::read_to_string(path)?;
        let jobs: Vec<CronJob> = serde_json::from_str(&data)?;
        let mut map = self.jobs.lock();
        for job in jobs {
            map.insert(job.id.clone(), job);
        }
        Ok(())
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
        let job = CronJob::new("job1", "0 * * * *", "echo hello");

        scheduler.add_job(job.clone()).unwrap();
        let fetched = scheduler.get_job("job1").unwrap();
        assert_eq!(fetched.id, "job1");
        assert!(fetched.continuity);
    }

    #[test]
    fn context_chain_a_to_b() {
        let scheduler = CronScheduler::new();
        let mut job_a = CronJob::new("job_a", "0 * * * *", "task_a");
        job_a.notepad = "results from job a".into();

        let mut job_b = CronJob::new("job_b", "0 * * * *", "task_b");
        job_b.context_from = Some("job_a".into());

        scheduler.add_job(job_a).unwrap();
        scheduler.add_job(job_b).unwrap();

        let context = scheduler.get_context_from("job_b").unwrap();
        assert_eq!(context, Some("job_a".to_string()));

        let built_context = scheduler.build_job_context("job_b", None).unwrap();
        assert!(built_context.contains("results from job a"));
    }

    #[test]
    fn cron_memory_continuity_and_notepad_across_runs() {
        let scheduler = CronScheduler::new();
        let job = CronJob::new("security-scan", "*/30 * * * *", "cargo audit");
        scheduler.add_job(job).unwrap();

        // Run 1: initial context has no prior notepad
        let ctx1 = scheduler.build_job_context("security-scan", None).unwrap();
        assert!(!ctx1.contains("[Notepad / Prior Continuity]"));

        // Append finding from Run 1
        scheduler
            .append_notepad(
                "security-scan",
                "Found 2 vulnerable dependencies: crate-a, crate-b",
            )
            .unwrap();

        // Run 2: context builder includes previous findings
        let ctx2 = scheduler.build_job_context("security-scan", None).unwrap();
        assert!(ctx2.contains("[Notepad / Prior Continuity]"));
        assert!(ctx2.contains("Found 2 vulnerable dependencies: crate-a, crate-b"));
    }

    #[test]
    fn cron_monitor_mode_skips_model_when_payload_unchanged() {
        let scheduler = CronScheduler::new();
        let job = CronJob::new("repo-monitor", "*/5 * * * *", "git fetch");
        scheduler.add_job(job).unwrap();

        let payload1 = "commit: abc1234, branch: main";
        let should_run1 = scheduler
            .check_and_update_hash("repo-monitor", payload1)
            .unwrap();
        assert!(should_run1, "first run should trigger model call");

        // Second run with exact same payload -> skipped
        let should_run2 = scheduler
            .check_and_update_hash("repo-monitor", payload1)
            .unwrap();
        assert!(!should_run2, "identical payload should skip model call");

        // Third run with changed payload -> triggered
        let payload2 = "commit: def5678, branch: main";
        let should_run3 = scheduler
            .check_and_update_hash("repo-monitor", payload2)
            .unwrap();
        assert!(should_run3, "changed payload should trigger model call");
    }

    #[test]
    fn cron_persistence_roundtrip() {
        let dir = std::env::temp_dir().join(format!("darius_cron_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let scheduler = CronScheduler::new();
        let mut job = CronJob::new("cron1", "0 0 * * *", "backup_db");
        job.notepad = "last backup completed at 12:00".into();
        scheduler.add_job(job).unwrap();

        scheduler.save_to_dir(&dir).unwrap();

        let scheduler2 = CronScheduler::new();
        scheduler2.load_from_dir(&dir).unwrap();
        let loaded = scheduler2.get_job("cron1").unwrap();
        assert_eq!(loaded.notepad, "last backup completed at 12:00");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn circuit_breaker_disables_after_max_failures() {
        let scheduler = CronScheduler::new().with_max_failures(3);
        let job = CronJob::new("failing", "0 * * * *", "fail");
        scheduler.add_job(job).unwrap();

        for _ in 0..3 {
            scheduler.record_run("failing", false).unwrap();
        }

        assert!(scheduler.is_circuit_broken("failing"));
        let job = scheduler.get_job("failing").unwrap();
        assert!(!job.enabled);
    }
}
