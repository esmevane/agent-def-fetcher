use agent_defs::{DefinitionKind, Source};
use anyhow::Result;

use super::format;

pub async fn run(
    sources: &[Box<dyn Source>],
    query: &str,
    kind_filter: Option<&str>,
    source_filter: Option<&str>,
) -> Result<()> {
    let kind_predicate = kind_filter.map(DefinitionKind::parse);
    let mut all = Vec::new();

    for source in sources {
        if let Some(filter) = source_filter
            && source.label() != filter
        {
            continue;
        }

        let results = source.search(query).await?;

        for summary in results {
            if let Some(ref target_kind) = kind_predicate
                && &summary.kind != target_kind
            {
                continue;
            }

            all.push(summary);
        }
    }

    if all.is_empty() {
        println!("No results found for \"{query}\".");
    } else {
        format::print_summary_table(&all);
    }

    Ok(())
}
