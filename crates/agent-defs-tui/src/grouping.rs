use agent_defs::{DefinitionKind, DefinitionSummary};

/// A group of definitions sharing the same kind.
#[derive(Debug, Clone)]
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
    use agent_defs::DefinitionId;

    use super::*;

    fn summary(name: &str, kind: DefinitionKind) -> DefinitionSummary {
        DefinitionSummary {
            id: DefinitionId::new(name),
            name: name.to_owned(),
            description: None,
            kind,
            category: None,
            source_label: "test".into(),
        }
    }

    #[test]
    fn empty_summaries_produce_no_groups() {
        let (groups, flat) = build_groups(&[]);
        assert!(groups.is_empty());
        assert!(flat.is_empty());
    }

    #[test]
    fn groups_sorted_by_kind() {
        let summaries = vec![
            summary("x", DefinitionKind::Skill),
            summary("y", DefinitionKind::Agent),
            summary("z", DefinitionKind::Hook),
        ];

        let (groups, _) = build_groups(&summaries);
        let labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, vec!["Agents", "Hooks", "Skills"]);
    }

    #[test]
    fn flat_items_interleave_headers_and_items() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Agent),
            summary("c", DefinitionKind::Hook),
        ];

        let (_, flat) = build_groups(&summaries);

        // Header(Agents), Item(a), Item(b), Header(Hooks), Item(c)
        assert_eq!(flat.len(), 5);
        assert!(matches!(flat[0], ListRow::Header { .. }));
        assert!(matches!(flat[1], ListRow::Item { .. }));
        assert!(matches!(flat[2], ListRow::Item { .. }));
        assert!(matches!(flat[3], ListRow::Header { .. }));
        assert!(matches!(flat[4], ListRow::Item { .. }));
    }

    #[test]
    fn first_item_index_skips_header() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let (_, flat) = build_groups(&summaries);

        assert_eq!(first_item_index(&flat), Some(1));
    }

    #[test]
    fn first_item_index_none_for_empty() {
        assert_eq!(first_item_index(&[]), None);
    }

    #[test]
    fn next_item_skips_headers() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Hook),
        ];
        let (_, flat) = build_groups(&summaries);

        // flat: Header(Agents), Item(a=idx1), Header(Hooks), Item(b=idx3)
        assert_eq!(next_item_index(&flat, 1), 3);
    }

    #[test]
    fn next_item_stays_at_end() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let (_, flat) = build_groups(&summaries);

        // flat: Header, Item(idx=1)
        assert_eq!(next_item_index(&flat, 1), 1);
    }

    #[test]
    fn prev_item_skips_headers() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Hook),
        ];
        let (_, flat) = build_groups(&summaries);

        // flat: Header(0), Item(1), Header(2), Item(3)
        assert_eq!(prev_item_index(&flat, 3), 1);
    }

    #[test]
    fn prev_item_stays_at_beginning() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let (_, flat) = build_groups(&summaries);

        assert_eq!(prev_item_index(&flat, 1), 1);
    }

    #[test]
    fn group_counts_match_entries() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Agent),
            summary("c", DefinitionKind::Agent),
            summary("d", DefinitionKind::Hook),
        ];

        let (groups, _) = build_groups(&summaries);
        assert_eq!(groups[0].count, 3); // Agents
        assert_eq!(groups[1].count, 1); // Hooks
    }

    #[test]
    fn kind_labels_are_plural() {
        assert_eq!(kind_label(&DefinitionKind::Agent), "Agents");
        assert_eq!(kind_label(&DefinitionKind::Skill), "Skills");
        assert_eq!(kind_label(&DefinitionKind::Mcp), "MCP Servers");
    }
}
