//! Grouping logic for organizing definitions by kind.
//! Ported from agent-defs-tui.

use agent_defs::{DefinitionKind, DefinitionSummary};

/// A group of definitions sharing the same kind.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used for future features (e.g., group actions)
pub struct Group {
    pub kind: DefinitionKind,
    pub label: String,
    pub count: usize,
    /// Indices into the original summaries vec.
    pub summary_indices: Vec<usize>,
}

/// A row in the flattened list: either a section header or a selectable item.
#[derive(Debug, Clone)]
pub enum ListRow {
    Header { label: String, count: usize },
    Item { summary_index: usize },
}

/// Human-readable plural label for a definition kind.
pub fn kind_label(kind: &DefinitionKind) -> &str {
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

/// Sort key for consistent kind ordering.
pub fn kind_sort_key(kind: &DefinitionKind) -> u8 {
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

/// Build sorted groups from summaries, returning both the groups and a
/// flattened list of rows for cursor navigation.
pub fn build_groups(summaries: &[DefinitionSummary]) -> (Vec<Group>, Vec<ListRow>) {
    let mut raw_groups: Vec<(DefinitionKind, Vec<usize>)> = Vec::new();

    for (idx, summary) in summaries.iter().enumerate() {
        if let Some(group) = raw_groups.iter_mut().find(|(k, _)| k == &summary.kind) {
            group.1.push(idx);
        } else {
            raw_groups.push((summary.kind.clone(), vec![idx]));
        }
    }

    raw_groups.sort_by_key(|(k, _)| kind_sort_key(k));

    let mut groups = Vec::new();
    let mut flat_items = Vec::new();

    for (kind, indices) in raw_groups {
        let label = kind_label(&kind).to_owned();
        let count = indices.len();

        flat_items.push(ListRow::Header {
            label: label.clone(),
            count,
        });

        for &idx in &indices {
            flat_items.push(ListRow::Item { summary_index: idx });
        }

        groups.push(Group {
            kind,
            label,
            count,
            summary_indices: indices,
        });
    }

    (groups, flat_items)
}

/// Find the first selectable (Item) row index, or None if empty.
pub fn first_item_index(flat_items: &[ListRow]) -> Option<usize> {
    flat_items
        .iter()
        .position(|row| matches!(row, ListRow::Item { .. }))
}

/// Find the next selectable row after `current`, or stay put.
pub fn next_item_index(flat_items: &[ListRow], current: usize) -> usize {
    flat_items
        .iter()
        .enumerate()
        .skip(current + 1)
        .find(|(_, row)| matches!(row, ListRow::Item { .. }))
        .map(|(i, _)| i)
        .unwrap_or(current)
}

/// Find the previous selectable row before `current`, or stay put.
pub fn prev_item_index(flat_items: &[ListRow], current: usize) -> usize {
    flat_items
        .iter()
        .enumerate()
        .take(current)
        .rev()
        .find(|(_, row)| matches!(row, ListRow::Item { .. }))
        .map(|(i, _)| i)
        .unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_defs::DefinitionId;

    fn make_summary(name: &str, kind: DefinitionKind) -> DefinitionSummary {
        DefinitionSummary {
            id: DefinitionId::new(format!("test:{name}")),
            name: name.to_string(),
            kind,
            description: Some(format!("Description for {name}")),
            category: None,
            source_label: "test".to_string(),
        }
    }

    #[test]
    fn test_build_groups_empty() {
        let (groups, flat_items) = build_groups(&[]);
        assert!(groups.is_empty());
        assert!(flat_items.is_empty());
    }

    #[test]
    fn test_build_groups_single_kind() {
        let summaries = vec![
            make_summary("agent1", DefinitionKind::Agent),
            make_summary("agent2", DefinitionKind::Agent),
        ];

        let (groups, flat_items) = build_groups(&summaries);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, DefinitionKind::Agent);
        assert_eq!(groups[0].count, 2);

        // flat_items: Header, Item, Item
        assert_eq!(flat_items.len(), 3);
        assert!(
            matches!(&flat_items[0], ListRow::Header { label, count } if label == "Agents" && *count == 2)
        );
        assert!(matches!(&flat_items[1], ListRow::Item { summary_index: 0 }));
        assert!(matches!(&flat_items[2], ListRow::Item { summary_index: 1 }));
    }

    #[test]
    fn test_build_groups_multiple_kinds_sorted() {
        let summaries = vec![
            make_summary("skill1", DefinitionKind::Skill),
            make_summary("agent1", DefinitionKind::Agent),
            make_summary("command1", DefinitionKind::Command),
        ];

        let (groups, flat_items) = build_groups(&summaries);

        // Groups should be sorted: Agent, Command, Skill
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].kind, DefinitionKind::Agent);
        assert_eq!(groups[1].kind, DefinitionKind::Command);
        assert_eq!(groups[2].kind, DefinitionKind::Skill);

        // flat_items: Header(Agents), Item, Header(Commands), Item, Header(Skills), Item
        assert_eq!(flat_items.len(), 6);
        assert!(matches!(&flat_items[0], ListRow::Header { label, .. } if label == "Agents"));
        assert!(matches!(&flat_items[1], ListRow::Item { summary_index: 1 })); // agent1 was at index 1
        assert!(matches!(&flat_items[2], ListRow::Header { label, .. } if label == "Commands"));
        assert!(matches!(&flat_items[3], ListRow::Item { summary_index: 2 })); // command1 was at index 2
        assert!(matches!(&flat_items[4], ListRow::Header { label, .. } if label == "Skills"));
        assert!(matches!(&flat_items[5], ListRow::Item { summary_index: 0 })); // skill1 was at index 0
    }

    #[test]
    fn test_first_item_index() {
        let flat_items = vec![
            ListRow::Header {
                label: "Test".into(),
                count: 1,
            },
            ListRow::Item { summary_index: 0 },
        ];

        assert_eq!(first_item_index(&flat_items), Some(1));
        assert_eq!(first_item_index(&[]), None);
    }

    #[test]
    fn test_next_item_index() {
        let flat_items = vec![
            ListRow::Header {
                label: "A".into(),
                count: 2,
            },
            ListRow::Item { summary_index: 0 },
            ListRow::Item { summary_index: 1 },
            ListRow::Header {
                label: "B".into(),
                count: 1,
            },
            ListRow::Item { summary_index: 2 },
        ];

        assert_eq!(next_item_index(&flat_items, 1), 2); // item 0 -> item 1
        assert_eq!(next_item_index(&flat_items, 2), 4); // item 1 -> item 2 (skips header)
        assert_eq!(next_item_index(&flat_items, 4), 4); // last item stays put
    }

    #[test]
    fn test_prev_item_index() {
        let flat_items = vec![
            ListRow::Header {
                label: "A".into(),
                count: 2,
            },
            ListRow::Item { summary_index: 0 },
            ListRow::Item { summary_index: 1 },
            ListRow::Header {
                label: "B".into(),
                count: 1,
            },
            ListRow::Item { summary_index: 2 },
        ];

        assert_eq!(prev_item_index(&flat_items, 4), 2); // item 2 -> item 1 (skips header)
        assert_eq!(prev_item_index(&flat_items, 2), 1); // item 1 -> item 0
        assert_eq!(prev_item_index(&flat_items, 1), 1); // first item stays put
    }
}
