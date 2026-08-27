//! Skill registry, Curator lifecycle, and loader.

use serde::{Deserialize, Serialize};

/// Skill source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillSource {
    BuiltIn,
    User,
    AgentCreated,
    Archived,
}

/// A skill with lifecycle metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub source: SkillSource,
    pub created_at: u64,
    pub last_used_at: u64,
    pub use_count: u64,
    pub view_count: u64,
    pub patch_count: u64,
    pub pinned: bool,
}

/// Curator metrics.
#[derive(Debug, Clone, Default)]
pub struct CuratorMetrics {
    pub archived_count: u64,
    pub pinned_count: u64,
    pub total_skills: u64,
}

/// Skill registry — manages a collection of skills.
#[derive(Debug, Default)]
pub struct SkillRegistry {
    skills: Vec<Skill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    pub fn add(&mut self, skill: Skill) {
        self.skills.push(skill);
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Skill> {
        self.skills.iter_mut().find(|s| s.id == id)
    }

    pub fn remove(&mut self, id: &str) -> Option<Skill> {
        if let Some(pos) = self.skills.iter().position(|s| s.id == id) {
            Some(self.skills.remove(pos))
        } else {
            None
        }
    }

    pub fn list(&self) -> &[Skill] {
        &self.skills
    }

    pub fn list_mut(&mut self) -> &mut Vec<Skill> {
        &mut self.skills
    }
}

/// Skill loader — loads skills from SKILL.md manifests.
pub struct SkillLoader;

impl SkillLoader {
    pub fn load(_skill_md: &str) -> Result<Skill, Box<dyn std::error::Error>> {
        Ok(Skill {
            id: "stub".into(),
            name: "stub".into(),
            source: SkillSource::User,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
        })
    }
}

/// Skill Curator — manages skill lifecycle, archival, and consolidation.
pub struct SkillCurator {
    registry: SkillRegistry,
    /// Staleness threshold in seconds (default: 30 days).
    staleness_threshold: u64,
    /// Archive directory path.
    archive_dir: String,
}

impl SkillCurator {
    pub fn new(archive_dir: impl Into<String>) -> Self {
        Self {
            registry: SkillRegistry::new(),
            staleness_threshold: 30 * 24 * 60 * 60, // 30 days
            archive_dir: archive_dir.into(),
        }
    }

    /// Set custom staleness threshold.
    pub fn with_staleness_threshold(mut self, threshold_secs: u64) -> Self {
        self.staleness_threshold = threshold_secs;
        self
    }

    /// Get the registry.
    pub fn registry(&self) -> &SkillRegistry {
        &self.registry
    }

    /// Get mutable registry.
    pub fn registry_mut(&mut self) -> &mut SkillRegistry {
        &mut self.registry
    }

    /// Add a skill.
    pub fn add_skill(&mut self, skill: Skill) {
        self.registry.add(skill);
    }

    /// Pin a skill (exempts from archival).
    pub fn pin(&mut self, id: &str) -> Result<(), String> {
        let skill = self.registry.get_mut(id).ok_or_else(|| format!("skill {id} not found"))?;
        skill.pinned = true;
        Ok(())
    }

    /// Unpin a skill.
    pub fn unpin(&mut self, id: &str) -> Result<(), String> {
        let skill = self.registry.get_mut(id).ok_or_else(|| format!("skill {id} not found"))?;
        skill.pinned = false;
        Ok(())
    }

    /// Check if a skill is stale.
    pub fn is_stale(&self, skill: &Skill, now: u64) -> bool {
        if skill.pinned {
            return false;
        }
        if skill.source != SkillSource::AgentCreated {
            return false;
        }
        now.saturating_sub(skill.last_used_at) > self.staleness_threshold
    }

    /// Auto-archive stale agent-created skills.
    pub fn auto_archive(&mut self, now: u64) -> Vec<Skill> {
        let mut archived = Vec::new();
        let mut to_archive_ids = Vec::new();

        for skill in self.registry.list() {
            if self.is_stale(skill, now) {
                to_archive_ids.push(skill.id.clone());
            }
        }

        for id in to_archive_ids {
            if let Some(skill) = self.registry.get_mut(&id) {
                skill.source = SkillSource::Archived;
                // In a real implementation, we'd create a tar.gz backup here.
                archived.push(skill.clone());
            }
        }

        archived
    }

    /// Get curator metrics.
    pub fn metrics(&self) -> CuratorMetrics {
        let mut metrics = CuratorMetrics::default();
        for skill in self.registry.list() {
            metrics.total_skills += 1;
            if skill.source == SkillSource::Archived {
                metrics.archived_count += 1;
            }
            if skill.pinned {
                metrics.pinned_count += 1;
            }
        }
        metrics
    }

    /// Consolidate skills (opt-in via aux model).
    pub fn consolidate(&self) -> Vec<String> {
        // In a real implementation, this would invoke an aux model to suggest
        // which skills to consolidate. For now, return a list of skill names.
        self.registry.list().iter().map(|s| s.name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_adds_and_lists() {
        let mut reg = SkillRegistry::new();
        assert_eq!(reg.len(), 0);

        reg.add(Skill {
            id: "s1".into(),
            name: "skill1".into(),
            source: SkillSource::User,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
        });

        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("s1").unwrap().name, "skill1");
    }

    #[test]
    fn curator_pin_exempts_from_archival() {
        let now = 1_000_000;
        let mut curator = SkillCurator::new("/tmp/archives")
            .with_staleness_threshold(100);

        curator.add_skill(Skill {
            id: "pinned".into(),
            name: "pinned-skill".into(),
            source: SkillSource::AgentCreated,
            created_at: 0,
            last_used_at: 0, // stale (0 + 100 < 1_000_000)
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
        });

        curator.pin("pinned").unwrap();
        let archived = curator.auto_archive(now);
        assert!(archived.is_empty());

        let metrics = curator.metrics();
        assert_eq!(metrics.archived_count, 0);
        assert_eq!(metrics.pinned_count, 1);
    }

    #[test]
    fn curator_auto_archives_stale_skills() {
        let now = 1_000_000;
        let mut curator = SkillCurator::new("/tmp/archives")
            .with_staleness_threshold(100);

        curator.add_skill(Skill {
            id: "fresh".into(),
            name: "fresh-skill".into(),
            source: SkillSource::AgentCreated,
            created_at: 0,
            last_used_at: now - 50, // fresh (within threshold)
            use_count: 10,
            view_count: 0,
            patch_count: 0,
            pinned: false,
        });

        curator.add_skill(Skill {
            id: "stale".into(),
            name: "stale-skill".into(),
            source: SkillSource::AgentCreated,
            created_at: 0,
            last_used_at: 0, // stale (0 + 100 < 1_000_000)
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
        });

        let archived = curator.auto_archive(now);
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, "stale");

        let metrics = curator.metrics();
        assert_eq!(metrics.archived_count, 1);
    }

    #[test]
    fn curator_does_not_archive_builtin_or_user_skills() {
        let now = 1_000_000;
        let mut curator = SkillCurator::new("/tmp/archives")
            .with_staleness_threshold(100);

        curator.add_skill(Skill {
            id: "builtin".into(),
            name: "builtin-skill".into(),
            source: SkillSource::BuiltIn,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
        });

        curator.add_skill(Skill {
            id: "user".into(),
            name: "user-skill".into(),
            source: SkillSource::User,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
        });

        let archived = curator.auto_archive(now);
        assert!(archived.is_empty());
    }
}
