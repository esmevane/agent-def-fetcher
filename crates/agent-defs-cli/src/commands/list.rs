use agent_defs::{DefinitionKind, Source};
use anyhow::Result;

use super::format;

pub async fn run(
    sources: &[Box<dyn Source>],
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

        let summaries = source.list().await?;

        for summary in summaries {
            if let Some(ref target_kind) = kind_predicate
                && &summary.kind != target_kind
            {
                continue;
            }

            all.push(summary);
        }
    }

    format::print_summary_table(&all);

    Ok(())
}
