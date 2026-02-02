use crate::DefinitionKind;

/// True if a relative path represents a definition file we care about.
/// Excludes hidden directories (segments starting with `.`).
pub fn is_definition_file(relative_path: &str) -> bool {
    if relative_path
        .split('/')
        .any(|segment| segment.starts_with('.'))
    {
        return false;
    }

    relative_path.ends_with(".md") || relative_path.ends_with(".json")
}

/// True if this relative path is a skill entry point: `skills/<category>/<name>/SKILL.md`.
pub fn is_skill_entry_point(relative_path: &str) -> bool {
    relative_path.starts_with("skills/") && relative_path.ends_with("/SKILL.md")
}

/// True if this relative path is under `skills/` but is NOT a SKILL.md entry point.
/// These are reference files that should be excluded from listing.
pub fn is_skill_reference(relative_path: &str) -> bool {
    relative_path.starts_with("skills/") && !relative_path.ends_with("/SKILL.md")
}

/// True if this ID represents a skill directory (no file extension).
pub fn is_skill_directory_id(relative_id: &str) -> bool {
    relative_id.starts_with("skills/")
        && !relative_id.ends_with(".md")
        && !relative_id.ends_with(".json")
}

/// Extract name, kind, and category from a skill entry point path.
/// Input: `skills/<category>/<name>/SKILL.md`
/// Output: (name, Skill, Some(category))
pub fn parse_skill_path(relative_path: &str) -> (String, DefinitionKind, Option<String>) {
    let parts: Vec<&str> = relative_path.split('/').collect();

    match parts.as_slice() {
        // skills/<category>/<name>/SKILL.md
        [_skills, category, name, _skill_md] => (
            (*name).to_owned(),
            DefinitionKind::Skill,
            Some((*category).to_owned()),
        ),
        _ => {
            // Fallback: strip trailing /SKILL.md, use last segment as name
            let dir = relative_path
                .strip_suffix("/SKILL.md")
                .unwrap_or(relative_path);
            let name = dir.rsplit('/').next().unwrap_or("unknown").to_owned();
            (name, DefinitionKind::Skill, None)
        }
    }
}

/// Extract definition name, kind, and category from a path relative to base_path.
///
/// Expected path structures:
/// - `agents/<category>/<name>.md` -> kind=Agent, category=Some(category)
/// - `hooks/<name>.md` -> kind=Hook, category=None
/// - `commands/<name>.md` -> kind=Command, category=None
/// - `<name>.md` -> kind=Other("unknown"), category=None
pub fn parse_relative_path(relative_path: &str) -> (String, DefinitionKind, Option<String>) {
    let parts: Vec<&str> = relative_path.split('/').collect();

    let file_name = parts.last().unwrap_or(&"unknown");
    let name = file_name
        .strip_suffix(".md")
        .or_else(|| file_name.strip_suffix(".json"))
        .unwrap_or(file_name)
        .to_owned();

    match parts.as_slice() {
        // e.g., agents/development-team/code-architect.md
        [kind_str, category, _file] => {
            let kind = DefinitionKind::parse(kind_str);
            (name, kind, Some((*category).to_owned()))
        }
        // e.g., hooks/pre-commit-lint.md
        [kind_str, _file] => {
            let kind = DefinitionKind::parse(kind_str);
            (name, kind, None)
        }
        // e.g., README.md (single file at root)
        [_file] => (name, DefinitionKind::Other("unknown".into()), None),
        // Deeper nesting: use first segment as kind, second as category
        [kind_str, category, ..] => {
            let kind = DefinitionKind::parse(kind_str);
            (name, kind, Some((*category).to_owned()))
        }
        _ => (name, DefinitionKind::Other("unknown".into()), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- is_definition_file --

    #[test]
    fn markdown_file_is_definition() {
        assert!(is_definition_file("agents/code-architect.md"));
    }

    #[test]
    fn json_file_is_definition() {
        assert!(is_definition_file("agents/data.json"));
    }

    #[test]
    fn non_md_json_file_is_not_definition() {
        assert!(!is_definition_file("agents/readme.txt"));
    }

    #[test]
    fn hidden_directory_excluded() {
        assert!(!is_definition_file(".claude-plugin/marketplace.json"));
    }

    #[test]
    fn nested_hidden_directory_excluded() {
        assert!(!is_definition_file("agents/.hidden/secret.md"));
    }

    // -- is_skill_entry_point --

    #[test]
    fn skill_entry_point_detected() {
        assert!(is_skill_entry_point("skills/ai-research/agents-crewai/SKILL.md"));
    }

    #[test]
    fn non_skill_path_not_entry_point() {
        assert!(!is_skill_entry_point("agents/code-architect.md"));
    }

    #[test]
    fn skill_reference_not_entry_point() {
        assert!(!is_skill_entry_point(
            "skills/ai-research/agents-crewai/references/crew-setup.md"
        ));
    }

    // -- is_skill_reference --

    #[test]
    fn skill_reference_detected() {
        assert!(is_skill_reference(
            "skills/ai-research/agents-crewai/references/crew-setup.md"
        ));
    }

    #[test]
    fn skill_entry_point_not_reference() {
        assert!(!is_skill_reference("skills/ai-research/agents-crewai/SKILL.md"));
    }

    #[test]
    fn non_skill_path_not_reference() {
        assert!(!is_skill_reference("agents/code-architect.md"));
    }

    // -- is_skill_directory_id --

    #[test]
    fn skill_directory_id_detected() {
        assert!(is_skill_directory_id("skills/ai-research/agents-crewai"));
    }

    #[test]
    fn md_path_not_skill_directory_id() {
        assert!(!is_skill_directory_id("skills/ai-research/agents-crewai/SKILL.md"));
    }

    #[test]
    fn non_skills_prefix_not_skill_directory_id() {
        assert!(!is_skill_directory_id("agents/development-team"));
    }

    // -- parse_skill_path --

    #[test]
    fn parses_standard_skill_path() {
        let (name, kind, category) =
            parse_skill_path("skills/ai-research/agents-crewai/SKILL.md");
        assert_eq!(name, "agents-crewai");
        assert_eq!(kind, DefinitionKind::Skill);
        assert_eq!(category.as_deref(), Some("ai-research"));
    }

    #[test]
    fn parses_skill_path_fallback() {
        let (name, kind, _category) = parse_skill_path("skills/deep/nested/extra/SKILL.md");
        assert_eq!(kind, DefinitionKind::Skill);
        // Fallback: last segment before SKILL.md
        assert_eq!(name, "extra");
    }

    // -- parse_relative_path --

    #[test]
    fn parses_agent_with_category() {
        let (name, kind, category) =
            parse_relative_path("agents/development-team/code-architect.md");
        assert_eq!(name, "code-architect");
        assert_eq!(kind, DefinitionKind::Agent);
        assert_eq!(category.as_deref(), Some("development-team"));
    }

    #[test]
    fn parses_hook_without_category() {
        let (name, kind, category) = parse_relative_path("hooks/pre-commit-lint.md");
        assert_eq!(name, "pre-commit-lint");
        assert_eq!(kind, DefinitionKind::Hook);
        assert_eq!(category, None);
    }

    #[test]
    fn parses_command_without_category() {
        let (name, kind, category) = parse_relative_path("commands/deploy.md");
        assert_eq!(name, "deploy");
        assert_eq!(kind, DefinitionKind::Command);
        assert_eq!(category, None);
    }

    #[test]
    fn parses_root_file_as_unknown() {
        let (name, kind, category) = parse_relative_path("README.md");
        assert_eq!(name, "README");
        assert_eq!(kind, DefinitionKind::Other("unknown".into()));
        assert_eq!(category, None);
    }

    #[test]
    fn parses_json_extension() {
        let (name, kind, category) =
            parse_relative_path("agents/development-team/data.json");
        assert_eq!(name, "data");
        assert_eq!(kind, DefinitionKind::Agent);
        assert_eq!(category.as_deref(), Some("development-team"));
    }

    #[test]
    fn parses_deeply_nested_path() {
        let (name, kind, category) =
            parse_relative_path("agents/team/sub/deep/file.md");
        assert_eq!(name, "file");
        assert_eq!(kind, DefinitionKind::Agent);
        assert_eq!(category.as_deref(), Some("team"));
    }
}
