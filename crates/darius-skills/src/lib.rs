//! Skill registry, Curator lifecycle, loader, and find-skills.

use darius_skill_parser::{ParseError, Skill as ParsedSkill};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod curator;
pub mod loader;
pub mod registry;

/// Skill source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillSource {
    BuiltIn,
    User,
    AgentCreated,
    Archived,
    Discovered,
}

/// A skill with lifecycle metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub source: SkillSource,
    pub created_at: u64,
    pub last_used_at: u64,
    pub use_count: u64,
    pub view_count: u64,
    pub patch_count: u64,
    pub pinned: bool,
    pub body: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Skill {
    /// Create a new skill from a parsed SKILL.md.
    pub fn from_parsed(parsed: ParsedSkill, source: SkillSource) -> Self {
        let now = current_timestamp();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: parsed.name,
            description: parsed.description.unwrap_or_default(),
            version: parsed.version,
            source,
            created_at: now,
            last_used_at: now,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: parsed.body,
            metadata: parsed.metadata,
        }
    }

    /// Record a use of this skill.
    pub fn record_use(&mut self) {
        self.use_count += 1;
        self.last_used_at = current_timestamp();
    }

    /// Record a view of this skill.
    pub fn record_view(&mut self) {
        self.view_count += 1;
    }

    /// Record a patch to this skill.
    pub fn record_patch(&mut self) {
        self.patch_count += 1;
    }
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
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    pub fn add(&mut self, skill: Skill) {
        self.skills.insert(skill.id.clone(), skill);
    }

    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.skills.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Skill> {
        self.skills.get_mut(id)
    }

    pub fn remove(&mut self, id: &str) -> Option<Skill> {
        self.skills.remove(id)
    }

    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Skill> {
        self.skills.values().find(|s| s.name == name)
    }

    pub fn find_by_query(&self, query: &str) -> Vec<&Skill> {
        let query_lower = query.to_lowercase();
        self.skills
            .values()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

/// Skill loader — loads skills from SKILL.md manifests.
pub struct SkillLoader;

impl SkillLoader {
    /// Load a skill from a SKILL.md string.
    pub fn load(skill_md: &str) -> Result<Skill, ParseError> {
        let parsed = darius_skill_parser::parse(skill_md)?;
        Ok(Skill::from_parsed(parsed, SkillSource::User))
    }

    /// Load a skill with a specific source.
    pub fn load_with_source(skill_md: &str, source: SkillSource) -> Result<Skill, ParseError> {
        let parsed = darius_skill_parser::parse(skill_md)?;
        Ok(Skill::from_parsed(parsed, source))
    }
}

/// Skill Curator — manages skill lifecycle, archival, and consolidation.
pub struct SkillCurator {
    registry: SkillRegistry,
    staleness_threshold: u64,
    archive_dir: PathBuf,
}

impl SkillCurator {
    pub fn new(archive_dir: impl Into<String>) -> Self {
        Self {
            registry: SkillRegistry::new(),
            staleness_threshold: 30 * 24 * 60 * 60, // 30 days
            archive_dir: PathBuf::from(archive_dir.into()),
        }
    }

    pub fn with_staleness_threshold(mut self, threshold_secs: u64) -> Self {
        self.staleness_threshold = threshold_secs;
        self
    }

    pub fn registry(&self) -> &SkillRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut SkillRegistry {
        &mut self.registry
    }

    pub fn add_skill(&mut self, skill: Skill) {
        self.registry.add(skill);
    }

    pub fn pin(&mut self, id: &str) -> Result<(), String> {
        let skill = self
            .registry
            .get_mut(id)
            .ok_or_else(|| format!("skill {id} not found"))?;
        skill.pinned = true;
        Ok(())
    }

    pub fn unpin(&mut self, id: &str) -> Result<(), String> {
        let skill = self
            .registry
            .get_mut(id)
            .ok_or_else(|| format!("skill {id} not found"))?;
        skill.pinned = false;
        Ok(())
    }

    pub fn is_stale(&self, skill: &Skill, now: u64) -> bool {
        if skill.pinned {
            return false;
        }
        if skill.source != SkillSource::AgentCreated {
            return false;
        }
        now.saturating_sub(skill.last_used_at) > self.staleness_threshold
    }

    pub fn auto_archive(&mut self, now: u64) -> Result<Vec<Skill>, String> {
        let to_archive_ids: Vec<String> = self
            .registry
            .list()
            .into_iter()
            .filter(|skill| self.is_stale(skill, now))
            .map(|skill| skill.id.clone())
            .collect();

        if to_archive_ids.is_empty() {
            return Ok(Vec::new());
        }

        std::fs::create_dir_all(&self.archive_dir).map_err(|error| error.to_string())?;

        let mut archived = Vec::with_capacity(to_archive_ids.len());
        for id in &to_archive_ids {
            let mut skill = self
                .registry
                .get(id)
                .cloned()
                .ok_or_else(|| format!("skill {id} not found"))?;
            skill.source = SkillSource::Archived;
            let json = serde_json::to_string_pretty(&skill).map_err(|error| error.to_string())?;
            let temp_path = self.archive_dir.join(format!("{id}.json.tmp"));
            let final_path = self.archive_dir.join(format!("{id}.json"));
            std::fs::write(&temp_path, json).map_err(|error| error.to_string())?;
            if let Err(error) = std::fs::rename(&temp_path, &final_path) {
                let _ = std::fs::remove_file(temp_path);
                return Err(error.to_string());
            }
            archived.push(skill);
        }

        for skill in &archived {
            if let Some(stored) = self.registry.get_mut(&skill.id) {
                stored.source = SkillSource::Archived;
            }
        }

        Ok(archived)
    }

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

    pub fn consolidate(&self) -> Vec<String> {
        self.registry
            .list()
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }
}

/// Find skills — search for skills across multiple sources.
pub struct SkillFinder;

impl SkillFinder {
    /// Search for skills in the registry.
    pub fn search<'a>(registry: &'a SkillRegistry, query: &str) -> Vec<&'a Skill> {
        registry.find_by_query(query)
    }

    /// Discover skills from agentskills.io (stub).
    pub fn discover(_query: &str) -> Vec<Skill> {
        // Stub: in a real implementation, this would query agentskills.io.
        vec![]
    }

    /// Create a skill autonomously (stub).
    pub fn create_autonomous(name: &str, description: &str, body: &str) -> Skill {
        let parsed = ParsedSkill {
            name: name.into(),
            description: Some(description.into()),
            version: "0.1.0".into(),
            body: body.into(),
            metadata: HashMap::new(),
        };
        Skill::from_parsed(parsed, SkillSource::AgentCreated)
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
    fn registry_adds_and_finds() {
        let mut reg = SkillRegistry::new();
        reg.add(Skill {
            id: "s1".into(),
            name: "test".into(),
            description: "a test skill".into(),
            version: "1.0.0".into(),
            source: SkillSource::User,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: "body".into(),
            metadata: HashMap::new(),
        });

        assert_eq!(reg.len(), 1);
        assert_eq!(reg.find_by_name("test").unwrap().id, "s1");

        let results = reg.find_by_query("test");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn curator_pin_exempts_from_archival() {
        let now = 1_000_000;
        let mut curator = SkillCurator::new("/tmp/archives").with_staleness_threshold(100);

        curator.add_skill(Skill {
            id: "pinned".into(),
            name: "pinned-skill".into(),
            description: "".into(),
            version: "1.0.0".into(),
            source: SkillSource::AgentCreated,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: "".into(),
            metadata: HashMap::new(),
        });

        curator.pin("pinned").unwrap();
        let archived = curator.auto_archive(now).unwrap();
        assert!(archived.is_empty());

        let metrics = curator.metrics();
        assert_eq!(metrics.archived_count, 0);
        assert_eq!(metrics.pinned_count, 1);
    }

    #[test]
    fn curator_auto_archives_stale_skills() {
        let now = 1_000_000;
        let mut curator = SkillCurator::new("/tmp/archives").with_staleness_threshold(100);

        curator.add_skill(Skill {
            id: "fresh".into(),
            name: "fresh-skill".into(),
            description: "".into(),
            version: "1.0.0".into(),
            source: SkillSource::AgentCreated,
            created_at: 0,
            last_used_at: now - 50,
            use_count: 10,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: "".into(),
            metadata: HashMap::new(),
        });

        curator.add_skill(Skill {
            id: "stale".into(),
            name: "stale-skill".into(),
            description: "".into(),
            version: "1.0.0".into(),
            source: SkillSource::AgentCreated,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: "".into(),
            metadata: HashMap::new(),
        });

        let archived = curator.auto_archive(now).unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, "stale");

        let metrics = curator.metrics();
        assert_eq!(metrics.archived_count, 1);
    }

    #[test]
    fn curator_does_not_archive_builtin_or_user_skills() {
        let now = 1_000_000;
        let mut curator = SkillCurator::new("/tmp/archives").with_staleness_threshold(100);

        curator.add_skill(Skill {
            id: "builtin".into(),
            name: "builtin-skill".into(),
            description: "".into(),
            version: "1.0.0".into(),
            source: SkillSource::BuiltIn,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: "".into(),
            metadata: HashMap::new(),
        });

        curator.add_skill(Skill {
            id: "user".into(),
            name: "user-skill".into(),
            description: "".into(),
            version: "1.0.0".into(),
            source: SkillSource::User,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: "".into(),
            metadata: HashMap::new(),
        });

        let archived = curator.auto_archive(now).unwrap();
        assert!(archived.is_empty());
    }

    #[test]
    fn curator_does_not_mark_archived_when_backup_fails() {
        let now = 1_000_000;
        let archive_path = std::env::temp_dir().join(format!(
            "darius_skill_archive_file_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&archive_path, "not a directory").unwrap();
        let mut curator = SkillCurator::new(archive_path.to_string_lossy().into_owned())
            .with_staleness_threshold(100);
        curator.add_skill(Skill {
            id: "must-remain-active".into(),
            name: "must-remain-active".into(),
            description: String::new(),
            version: "1.0.0".into(),
            source: SkillSource::AgentCreated,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: "body".into(),
            metadata: HashMap::new(),
        });

        assert!(curator.auto_archive(now).is_err());
        assert_eq!(
            curator.registry().get("must-remain-active").unwrap().source,
            SkillSource::AgentCreated
        );
        assert_eq!(curator.metrics().archived_count, 0);

        std::fs::remove_file(archive_path).unwrap();
    }

    #[test]
    fn curator_archives_to_backup_without_deleting_registry_entry() {
        let now = 1_000_000;
        let archive_dir = std::env::temp_dir().join(format!(
            "darius_skill_archive_{}_{}",
            std::process::id(),
            current_timestamp()
        ));
        let mut curator = SkillCurator::new(archive_dir.to_string_lossy().into_owned())
            .with_staleness_threshold(100);
        curator.add_skill(Skill {
            id: "archived-but-retained".into(),
            name: "archived-but-retained".into(),
            description: String::new(),
            version: "1.0.0".into(),
            source: SkillSource::AgentCreated,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: "body".into(),
            metadata: HashMap::new(),
        });

        let archived = curator.auto_archive(now).unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(curator.registry().len(), 1);
        assert_eq!(
            curator
                .registry()
                .get("archived-but-retained")
                .unwrap()
                .source,
            SkillSource::Archived
        );
        assert!(archive_dir.join("archived-but-retained.json").is_file());

        std::fs::remove_dir_all(archive_dir).unwrap();
    }

    #[test]
    fn skill_from_parsed() {
        let parsed = ParsedSkill {
            name: "test-skill".into(),
            description: Some("A test".into()),
            version: "1.0.0".into(),
            body: "body content".into(),
            metadata: HashMap::new(),
        };

        let skill = Skill::from_parsed(parsed, SkillSource::User);
        assert_eq!(skill.name, "test-skill");
        assert_eq!(skill.description, "A test");
        assert_eq!(skill.body, "body content");
        assert_eq!(skill.source, SkillSource::User);
    }

    #[test]
    fn skill_finder_search() {
        let mut reg = SkillRegistry::new();
        reg.add(Skill {
            id: "s1".into(),
            name: "coding-helper".into(),
            description: "helps with coding".into(),
            version: "1.0.0".into(),
            source: SkillSource::User,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
            body: "".into(),
            metadata: HashMap::new(),
        });

        let results = SkillFinder::search(&reg, "coding");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn skill_finder_create_autonomous() {
        let skill = SkillFinder::create_autonomous("new-skill", "description", "body");
        assert_eq!(skill.name, "new-skill");
        assert_eq!(skill.source, SkillSource::AgentCreated);
    }
}
