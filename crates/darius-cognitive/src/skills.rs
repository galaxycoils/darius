//! SKILL.md injection for cognitive loop preface.

use std::path::Path;

const DEFAULT_SKILL_PREFACE_CAP: usize = 2000;

/// A skill that can be injected into the loop preface.
#[derive(Debug, Clone)]
pub struct SkillPreface {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// Find skills whose keywords match the goal.
///
/// In production, this would use `npx skills find` or the agentskills.io API.
/// For now, it uses a simple keyword-matching heuristic against a static list.
pub fn find_matching_skills(goal: &str, available_skills: &[SkillPreface]) -> Vec<SkillPreface> {
    let goal_lower = goal.to_lowercase();
    let goal_words: Vec<&str> = goal_lower
        .split_whitespace()
        .filter(|w| w.len() >= 2)
        .collect();

    available_skills
        .iter()
        .filter(|skill| {
            let name_lower = skill.name.to_lowercase();
            let desc_lower = skill.description.to_lowercase();

            goal_words
                .iter()
                .any(|word| name_lower.contains(word) || desc_lower.contains(word))
        })
        .cloned()
        .collect()
}

/// Build a capped skill preface string for injection into the loop.
pub fn build_skill_preface(skills: &[SkillPreface], max_chars: usize) -> String {
    let mut preface = String::new();

    for skill in skills {
        let entry = format!("## Skill: {}\n{}\n\n", skill.name, skill.body);

        if preface.len() + entry.len() > max_chars {
            break;
        }

        preface.push_str(&entry);
    }

    if preface.ends_with("\n\n") {
        preface.truncate(preface.len() - 2);
    }

    preface
}

/// Build a capped skill preface with default cap.
pub fn build_skill_preface_default(skills: &[SkillPreface]) -> String {
    build_skill_preface(skills, DEFAULT_SKILL_PREFACE_CAP)
}

/// Load skills from a directory (each .md file is a skill).
pub fn load_skills_from_dir(dir: &Path) -> std::io::Result<Vec<SkillPreface>> {
    let mut skills = Vec::new();

    if !dir.exists() {
        return Ok(skills);
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map_or(false, |e| e == "md") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                let (description, body) = parse_skill_md(&content);

                skills.push(SkillPreface {
                    name,
                    description,
                    body,
                });
            }
        }
    }

    Ok(skills)
}

fn parse_skill_md(content: &str) -> (String, String) {
    let mut lines = content.lines();
    let mut description = String::new();
    let mut in_frontmatter = false;
    let mut body_start = 0;

    for (i, line) in lines.enumerate() {
        if line.starts_with("---") {
            if !in_frontmatter {
                in_frontmatter = true;
                continue;
            } else {
                body_start = i + 1;
                break;
            }
        }

        if in_frontmatter {
            if line.starts_with("description:") {
                description = line
                    .strip_prefix("description:")
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"')
                    .to_string();
            }
        }
    }

    let body = content
        .lines()
        .skip(body_start)
        .collect::<Vec<_>>()
        .join("\n");
    (description, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_matching_skills_by_keyword() {
        let skills = vec![
            SkillPreface {
                name: "rust-debugging".into(),
                description: "Debug Rust programs".into(),
                body: "Use gdb or lldb".into(),
            },
            SkillPreface {
                name: "python-web".into(),
                description: "Build Python web apps".into(),
                body: "Use Flask or Django".into(),
            },
        ];

        let matching = find_matching_skills("fix rust bug", &skills);
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].name, "rust-debugging");

        let matching = find_matching_skills("build a website", &skills);
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].name, "python-web");
    }

    #[test]
    fn find_matching_skills_no_match() {
        let skills = vec![SkillPreface {
            name: "rust-debugging".into(),
            description: "Debug Rust programs".into(),
            body: "Use gdb or lldb".into(),
        }];

        let matching = find_matching_skills("cook a recipe", &skills);
        assert!(matching.is_empty());
    }

    #[test]
    fn build_skill_preface_respects_cap() {
        let skills = vec![
            SkillPreface {
                name: "skill1".into(),
                description: "desc1".into(),
                body: "a".repeat(500),
            },
            SkillPreface {
                name: "skill2".into(),
                description: "desc2".into(),
                body: "b".repeat(500),
            },
            SkillPreface {
                name: "skill3".into(),
                description: "desc3".into(),
                body: "c".repeat(500),
            },
        ];

        let preface = build_skill_preface(&skills, 1200);
        assert!(preface.len() <= 1200);
        assert!(preface.contains("skill1"));
        // With 1200 cap: skill1 (~516 chars) + skill2 (~516 chars) = ~1032, skill3 would exceed
    }

    #[test]
    fn load_skills_from_dir_loads_md_files() {
        let dir = std::env::temp_dir().join(format!("darius_skills_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let skill1 = dir.join("rust-debug.md");
        std::fs::write(
            &skill1,
            "---\ndescription: Debug Rust\n---\nUse gdb or lldb for debugging.",
        )
        .unwrap();

        let skill2 = dir.join("python-web.md");
        std::fs::write(
            &skill2,
            "---\ndescription: Build web apps\n---\nUse Flask or Django.",
        )
        .unwrap();

        let skills = load_skills_from_dir(&dir).unwrap();
        assert_eq!(skills.len(), 2);

        let names: Vec<String> = skills.iter().map(|s| s.name.clone()).collect();
        assert!(names.contains(&"rust-debug".to_string()));
        assert!(names.contains(&"python-web".to_string()));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_skills_from_nonexistent_dir_returns_empty() {
        let dir = std::env::temp_dir().join("darius_skills_nonexistent");
        let skills = load_skills_from_dir(&dir).unwrap();
        assert!(skills.is_empty());
    }
}
