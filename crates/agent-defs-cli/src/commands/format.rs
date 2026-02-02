use agent_defs::{DefinitionKind, DefinitionSummary};

const MAX_NAME_WIDTH: usize = 35;
const LINE_BUDGET: usize = 90;

pub fn print_summary_table(summaries: &[DefinitionSummary]) {
    if summaries.is_empty() {
        return;
    }

    let groups = group_by_kind(summaries);
    let mut total = 0usize;
    let mut first = true;

    for (kind, entries) in &groups {
        if !first {
            println!();
        }
        first = false;

        let name_width = entries
            .iter()
            .map(|s| s.name.chars().count())
            .max()
            .unwrap_or(0)
            .min(MAX_NAME_WIDTH);

        let desc_budget = LINE_BUDGET.saturating_sub(2 + name_width + 2);

        println!("{} ({})", kind_label(kind), entries.len());

        for entry in entries {
            let name = truncate(&entry.name, name_width);
            let desc = entry.description.as_deref().unwrap_or("");
            let desc = truncate(desc, desc_budget);

            println!("  {:<width$}  {}", name, desc, width = name_width);
        }

        total += entries.len();
    }

    println!("\n{total} definitions");
}

fn kind_label(kind: &DefinitionKind) -> &str {
    match kind {
        DefinitionKind::Agent => "Agents",
        DefinitionKind::Command => "Commands",
        DefinitionKind::Hook => "Hooks",
        DefinitionKind::Mcp => "MCP Servers",
        DefinitionKind::Setting => "Settings",
        DefinitionKind::Skill => "Skills",
        DefinitionKind::Other(s) => s.as_str(),
    }
}

fn kind_sort_key(kind: &DefinitionKind) -> u8 {
    match kind {
        DefinitionKind::Agent => 0,
        DefinitionKind::Command => 1,
        DefinitionKind::Hook => 2,
        DefinitionKind::Mcp => 3,
        DefinitionKind::Setting => 4,
        DefinitionKind::Skill => 5,
        DefinitionKind::Other(_) => 6,
    }
}

fn group_by_kind(summaries: &[DefinitionSummary]) -> Vec<(&DefinitionKind, Vec<&DefinitionSummary>)> {
    let mut groups: Vec<(&DefinitionKind, Vec<&DefinitionSummary>)> = Vec::new();

    for summary in summaries {
        if let Some(group) = groups.iter_mut().find(|(k, _)| *k == &summary.kind) {
            group.1.push(summary);
        } else {
            groups.push((&summary.kind, vec![summary]));
        }
    }

    groups.sort_by_key(|(k, _)| kind_sort_key(k));
    groups
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_owned()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string_adds_ellipsis() {
        assert_eq!(truncate("hello world", 6), "hello…");
    }

    #[test]
    fn truncate_handles_unicode() {
        assert_eq!(truncate("café latte", 5), "café…");
    }

    #[test]
    fn kind_labels_are_plural() {
        assert_eq!(kind_label(&DefinitionKind::Agent), "Agents");
        assert_eq!(kind_label(&DefinitionKind::Skill), "Skills");
        assert_eq!(kind_label(&DefinitionKind::Mcp), "MCP Servers");
    }

    #[test]
    fn groups_preserve_kind_order() {
        let summaries = vec![
            summary("a", DefinitionKind::Skill),
            summary("b", DefinitionKind::Agent),
            summary("c", DefinitionKind::Skill),
        ];

        let groups = group_by_kind(&summaries);
        let kinds: Vec<&str> = groups.iter().map(|(k, _)| kind_label(k)).collect();

        // Agent sorts before Skill regardless of input order
        assert_eq!(kinds, vec!["Agents", "Skills"]);
    }

    #[test]
    fn groups_collect_entries_correctly() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Agent),
            summary("c", DefinitionKind::Hook),
        ];

        let groups = group_by_kind(&summaries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].1.len(), 1);
    }

    fn summary(name: &str, kind: DefinitionKind) -> DefinitionSummary {
        DefinitionSummary {
            id: agent_defs::DefinitionId::new(name),
            name: name.to_owned(),
            description: None,
            kind,
            category: None,
            source_label: "test".into(),
        }
    }
}
