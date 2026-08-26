//! Continual Harness — persistent learning and refinement engine.

use std::collections::HashMap;

/// A refinement suggestion backed by evaluation evidence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Refinement {
    pub id: String,
    pub source: String,
    pub evidence: String,
    pub suggested_change: String,
    pub created_at: u64,
}

/// Snapshot of the current harness state for rollback.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HarnessSnapshot {
    pub version: u32,
    pub created_at: u64,
    pub supplemental_prompts: HashMap<String, String>,
    pub skills: Vec<String>,
    pub memories: Vec<String>,
    pub subagent_specs: HashMap<String, String>,
}

/// Continual Harness — manages persistent learning and refinement.
pub struct ContinualHarness {
    base_prompt: String,
    supplemental_prompts: HashMap<String, String>,
    skills: Vec<String>,
    memories: Vec<String>,
    subagent_specs: HashMap<String, String>,
    snapshots: Vec<HarnessSnapshot>,
}

impl ContinualHarness {
    /// Create a new harness with the immutable base prompt.
    pub fn new(base_prompt: String) -> Self {
        Self {
            base_prompt,
            supplemental_prompts: HashMap::new(),
            skills: Vec::new(),
            memories: Vec::new(),
            subagent_specs: HashMap::new(),
            snapshots: Vec::new(),
        }
    }

    /// Get the immutable base prompt.
    pub fn base_prompt(&self) -> &str {
        &self.base_prompt
    }

    /// Add a supplemental prompt (does not modify base prompt).
    pub fn add_supplemental_prompt(&mut self, key: String, prompt: String) {
        self.supplemental_prompts.insert(key, prompt);
    }

    /// Get a supplemental prompt.
    pub fn get_supplemental_prompt(&self, key: &str) -> Option<&String> {
        self.supplemental_prompts.get(key)
    }

    /// List all supplemental prompt keys.
    pub fn supplemental_prompt_keys(&self) -> Vec<String> {
        self.supplemental_prompts.keys().cloned().collect()
    }

    /// Add a skill (will be emitted to Curator).
    pub fn add_skill(&mut self, skill: String) {
        if !self.skills.contains(&skill) {
            self.skills.push(skill);
        }
    }

    /// List skills.
    pub fn skills(&self) -> &Vec<String> {
        &self.skills
    }

    /// Add a memory.
    pub fn add_memory(&mut self, memory: String) {
        self.memories.push(memory);
    }

    /// List memories.
    pub fn memories(&self) -> &Vec<String> {
        &self.memories
    }

    /// Add a subagent spec.
    pub fn add_subagent_spec(&mut self, name: String, spec: String) {
        self.subagent_specs.insert(name, spec);
    }

    /// Get a subagent spec.
    pub fn get_subagent_spec(&self, name: &str) -> Option<&String> {
        self.subagent_specs.get(name)
    }

    /// Create a snapshot for rollback.
    pub fn snapshot(&mut self) -> HarnessSnapshot {
        let snap = HarnessSnapshot {
            version: self.snapshots.len() as u32 + 1,
            created_at: current_timestamp(),
            supplemental_prompts: self.supplemental_prompts.clone(),
            skills: self.skills.clone(),
            memories: self.memories.clone(),
            subagent_specs: self.subagent_specs.clone(),
        };
        self.snapshots.push(snap.clone());
        snap
    }

    /// Rollback to a specific snapshot version.
    pub fn rollback(&mut self, version: u32) -> Result<(), String> {
        let snap = self
            .snapshots
            .iter()
            .find(|s| s.version == version)
            .ok_or_else(|| format!("snapshot version {version} not found"))?;

        self.supplemental_prompts = snap.supplemental_prompts.clone();
        self.skills = snap.skills.clone();
        self.memories = snap.memories.clone();
        self.subagent_specs = snap.subagent_specs.clone();
        Ok(())
    }

    /// List available snapshots.
    pub fn snapshots(&self) -> &Vec<HarnessSnapshot> {
        &self.snapshots
    }

    /// Apply a refinement (evidence-backed change).
    pub fn apply_refinement(&mut self, refinement: Refinement) -> Result<(), String> {
        match refinement.source.as_str() {
            "prompt" => {
                self.add_supplemental_prompt(refinement.id, refinement.suggested_change);
            }
            "skill" => {
                self.add_skill(refinement.suggested_change);
            }
            "memory" => {
                self.add_memory(refinement.suggested_change);
            }
            "subagent" => {
                self.add_subagent_spec(refinement.id, refinement.suggested_change);
            }
            _ => return Err(format!("unknown refinement source: {}", refinement.source)),
        }
        Ok(())
    }

    /// Ingest an eval/learn target.
    pub fn ingest_target(&mut self, target: String) {
        self.add_memory(format!("target:{target}"));
    }

    /// Emit skills to Curator (stub for now).
    pub fn emit_skills_to_curator(&self) -> Vec<String> {
        self.skills.clone()
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
    fn harness_creates_with_base_prompt() {
        let harness = ContinualHarness::new("base prompt".to_string());
        assert_eq!(harness.base_prompt(), "base prompt");
    }

    #[test]
    fn base_prompt_never_mutated() {
        let mut harness = ContinualHarness::new("immutable base".to_string());
        let original = harness.base_prompt().to_string();
        
        harness.add_supplemental_prompt("test".to_string(), "supplemental".to_string());
        harness.add_skill("skill1".to_string());
        harness.add_memory("memory1".to_string());
        harness.add_subagent_spec("agent1".to_string(), "spec1".to_string());
        
        assert_eq!(harness.base_prompt(), "immutable base");
        assert_eq!(harness.base_prompt(), original);
    }

    #[test]
    fn supplemental_prompts_crud() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        harness.add_supplemental_prompt("key1".to_string(), "prompt1".to_string());
        assert_eq!(harness.get_supplemental_prompt("key1"), Some(&"prompt1".to_string()));
        assert_eq!(harness.supplemental_prompt_keys().len(), 1);
        
        harness.add_supplemental_prompt("key2".to_string(), "prompt2".to_string());
        assert_eq!(harness.supplemental_prompt_keys().len(), 2);
    }

    #[test]
    fn skills_crud() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        harness.add_skill("skill1".to_string());
        harness.add_skill("skill2".to_string());
        assert_eq!(harness.skills().len(), 2);
        assert!(harness.skills().contains(&"skill1".to_string()));
        
        harness.add_skill("skill1".to_string());
        assert_eq!(harness.skills().len(), 2);
    }

    #[test]
    fn memories_crud() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        harness.add_memory("memory1".to_string());
        harness.add_memory("memory2".to_string());
        assert_eq!(harness.memories().len(), 2);
    }

    #[test]
    fn subagent_specs_crud() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        harness.add_subagent_spec("agent1".to_string(), "spec1".to_string());
        assert_eq!(harness.get_subagent_spec("agent1"), Some(&"spec1".to_string()));
        
        harness.add_subagent_spec("agent2".to_string(), "spec2".to_string());
        assert_eq!(harness.get_subagent_spec("agent2"), Some(&"spec2".to_string()));
        assert_eq!(harness.get_subagent_spec("agent3"), None);
    }

    #[test]
    fn snapshot_creates_and_lists() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        harness.add_skill("skill1".to_string());
        let snap1 = harness.snapshot();
        assert_eq!(snap1.version, 1);
        
        harness.add_skill("skill2".to_string());
        let snap2 = harness.snapshot();
        assert_eq!(snap2.version, 2);
        
        assert_eq!(harness.snapshots().len(), 2);
    }

    #[test]
    fn rollback_to_snapshot() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        harness.add_skill("skill1".to_string());
        harness.snapshot();
        
        harness.add_skill("skill2".to_string());
        harness.snapshot();
        
        harness.rollback(1).unwrap();
        assert_eq!(harness.skills().len(), 1);
        assert!(harness.skills().contains(&"skill1".to_string()));
        assert!(!harness.skills().contains(&"skill2".to_string()));
    }

    #[test]
    fn rollback_missing_version_fails() {
        let mut harness = ContinualHarness::new("base".to_string());
        assert!(harness.rollback(999).is_err());
    }

    #[test]
    fn refine_applies_correctly() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        let refinement = Refinement {
            id: "test-prompt".to_string(),
            source: "prompt".to_string(),
            evidence: "test evidence".to_string(),
            suggested_change: "new prompt".to_string(),
            created_at: current_timestamp(),
        };
        
        harness.apply_refinement(refinement).unwrap();
        assert_eq!(harness.get_supplemental_prompt("test-prompt"), Some(&"new prompt".to_string()));
    }

    #[test]
    fn refine_invalid_source_fails() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        let refinement = Refinement {
            id: "test".to_string(),
            source: "invalid".to_string(),
            evidence: "test".to_string(),
            suggested_change: "change".to_string(),
            created_at: current_timestamp(),
        };
        
        assert!(harness.apply_refinement(refinement).is_err());
    }

    #[test]
    fn ingest_target_adds_memory() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        harness.ingest_target("evaluation-target-1".to_string());
        assert_eq!(harness.memories().len(), 1);
        assert!(harness.memories().first().unwrap().contains("target:evaluation-target-1"));
    }

    #[test]
    fn emit_skills_returns_list() {
        let mut harness = ContinualHarness::new("base".to_string());
        
        harness.add_skill("skill1".to_string());
        harness.add_skill("skill2".to_string());
        
        let emitted = harness.emit_skills_to_curator();
        assert_eq!(emitted.len(), 2);
    }
}
