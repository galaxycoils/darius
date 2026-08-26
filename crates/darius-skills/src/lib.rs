//! Skill registry, Curator lifecycle, and loader.

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

pub enum SkillSource {
    BuiltIn,
    User,
    AgentCreated,
    Archived,
}

pub struct SkillRegistry {
    skills: Vec<Skill>,
}

pub struct CuratorMetrics {
    pub archived_count: u64,
    pub pinned_count: u64,
    pub total_skills: u64,
}

pub struct SkillLoader;

impl SkillRegistry {
    pub fn new() -> Self { Self { skills: vec![] } }
    pub fn add(&mut self, skill: Skill) { self.skills.push(skill); }
    pub fn len(&self) -> usize { self.skills.len() }
}

impl Default for CuratorMetrics {
    fn default() -> Self {
        Self { archived_count: 0, pinned_count: 0, total_skills: 0 }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_adds() {
        let mut reg = SkillRegistry::new();
        assert_eq!(reg.len(), 0);
        reg.add(Skill {
            id: "x".into(),
            name: "x".into(),
            source: SkillSource::User,
            created_at: 0,
            last_used_at: 0,
            use_count: 0,
            view_count: 0,
            patch_count: 0,
            pinned: false,
        });
        assert_eq!(reg.len(), 1);
    }
}
