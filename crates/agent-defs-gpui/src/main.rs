mod app;
mod grouping;

use std::path::PathBuf;
use std::sync::Arc;

use agent_defs::{CompositeSource, Source};
use agent_defs_store::DefinitionStore;
use anyhow::{Context, Result};
use gpui::{
    App, Application, Bounds, Menu, MenuItem, TitlebarOptions, WindowBounds, WindowOptions,
    actions, point, prelude::*, px, size,
};

use crate::app::AgentDefsApp;

actions!(
    agent_defs_gui,
    [
        Quit,
        Sync,
        MoveUp,
        MoveDown,
        EnterSearch,
        ExitSearch,
        ClearFilters,
        SelectItem,
        EnterKindFilter,
        EnterSourceFilter,
        Install,
        ToggleCommandPalette,
    ]
);

/// Known source labels in the database.
const SOURCE_LABELS: &[&str] = &["awesome-subagents", "claude-code-templates"];

fn cache_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir().context("could not determine cache directory")?;
    let dir = base.join("agent-def-fetcher");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create cache directory: {}", dir.display()))?;
    Ok(dir)
}

fn db_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("definitions.db"))
}

fn build_store(label: &str) -> Result<DefinitionStore> {
    let path = db_path()?;
    DefinitionStore::open(&path, label).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Build a composite source from all known source labels.
fn build_composite_source() -> Result<Arc<dyn Source>> {
    let stores: Vec<Arc<dyn Source>> = SOURCE_LABELS
        .iter()
        .filter_map(|label| build_store(label).ok())
        .map(|s| Arc::new(s) as Arc<dyn Source>)
        .collect();

    if stores.is_empty() {
        anyhow::bail!("No stores could be opened");
    }

    Ok(Arc::new(CompositeSource::new(stores)))
}

fn main() {
    Application::new().run(|cx: &mut App| {
        // Set up macOS menu bar
        cx.set_menus(vec![
            Menu {
                name: "Agent Defs".into(),
                items: vec![
                    MenuItem::action("About Agent Defs Browser", Quit), // TODO: proper About action
                    MenuItem::separator(),
                    MenuItem::action("Quit Agent Defs Browser", Quit),
                ],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("Sync Definitions", Sync),
                    MenuItem::action("Install Selected", Install),
                ],
            },
            Menu {
                name: "View".into(),
                items: vec![
                    MenuItem::action("Search", EnterSearch),
                    MenuItem::action("Filter by Kind", EnterKindFilter),
                    MenuItem::action("Filter by Source", EnterSourceFilter),
                    MenuItem::separator(),
                    MenuItem::action("Clear Filters", ClearFilters),
                ],
            },
        ]);

        // Bind keys - mode checking is done in action handlers to allow search input
        cx.bind_keys([
            gpui::KeyBinding::new("q", Quit, Some("AgentDefsApp")),
            gpui::KeyBinding::new("j", MoveDown, Some("AgentDefsApp")),
            gpui::KeyBinding::new("k", MoveUp, Some("AgentDefsApp")),
            gpui::KeyBinding::new("s", Sync, Some("AgentDefsApp")),
            gpui::KeyBinding::new("f", EnterKindFilter, Some("AgentDefsApp")),
            gpui::KeyBinding::new("p", EnterSourceFilter, Some("AgentDefsApp")),
            gpui::KeyBinding::new("i", Install, Some("AgentDefsApp")),
            gpui::KeyBinding::new("/", EnterSearch, Some("AgentDefsApp")),
            gpui::KeyBinding::new("down", MoveDown, Some("AgentDefsApp")),
            gpui::KeyBinding::new("up", MoveUp, Some("AgentDefsApp")),
            gpui::KeyBinding::new("escape", ExitSearch, Some("AgentDefsApp")),
            gpui::KeyBinding::new("enter", SelectItem, Some("AgentDefsApp")),
            gpui::KeyBinding::new("backspace", ClearFilters, Some("AgentDefsApp")),
            // Command palette - cmd+k on mac, ctrl+k elsewhere
            gpui::KeyBinding::new("cmd-k", ToggleCommandPalette, Some("AgentDefsApp")),
            gpui::KeyBinding::new("ctrl-k", ToggleCommandPalette, Some("AgentDefsApp")),
            // Standard macOS shortcuts
            gpui::KeyBinding::new("cmd-q", Quit, None),
        ]);

        cx.on_action(|_: &Quit, cx: &mut App| {
            cx.quit();
        });

        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Agent Defs Browser".into()),
                    appears_transparent: false,
                    traffic_light_position: Some(point(px(9.0), px(9.0))),
                }),
                focus: true,
                show: true,
                ..Default::default()
            },
            |_window, cx| {
                // Build composite source from all known labels
                let source = match build_composite_source() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to open stores: {e}");
                        panic!("Failed to open stores: {e}");
                    }
                };

                cx.new(|cx| AgentDefsApp::new(source, cx))
            },
        )
        .expect("Failed to open window");

        // Bring app to foreground
        cx.activate(true);
    });
}
