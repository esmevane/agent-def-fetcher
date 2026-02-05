//! Main application state and rendering for the GPUI agent definition browser.

use std::sync::Arc;

use agent_defs::{Definition, DefinitionId, DefinitionKind, DefinitionSummary, Source};
use gpui::{
    App, AsyncApp, Context, CursorStyle, Entity, FocusHandle, Focusable, IntoElement,
    ListAlignment, ListState, ParentElement, Render, Styled, WeakEntity, Window, div, list,
    prelude::*, px,
};

use crate::grouping::{self, Group, ListRow};
use crate::{
    ClearFilters, EnterKindFilter, EnterSearch, EnterSourceFilter, ExitSearch, Install, MoveDown,
    MoveUp, Quit, SelectItem, Sync as SyncAction, ToggleCommandPalette,
};

/// Drag data for resize handle.
#[derive(Clone)]
struct ResizeHandleDrag {
    start_width: f32,
}

/// Empty view used as drag visual (we don't need to show anything for resize).
struct EmptyDragView {
    _start_width: f32,
}

impl Render for EmptyDragView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Return an invisible element
        div().w(px(0.0)).h(px(0.0))
    }
}

/// Colors - Catppuccin Mocha theme
mod colors {
    use gpui::Rgba;
    use gpui::rgb;

    pub fn base() -> Rgba {
        rgb(0x1e1e2e)
    }
    pub fn surface0() -> Rgba {
        rgb(0x313244)
    }
    pub fn surface1() -> Rgba {
        rgb(0x45475a)
    }
    #[allow(dead_code)]
    pub fn surface2() -> Rgba {
        rgb(0x585b70)
    }
    pub fn text() -> Rgba {
        rgb(0xcdd6f4)
    }
    pub fn subtext0() -> Rgba {
        rgb(0xa6adc8)
    }
    pub fn subtext1() -> Rgba {
        rgb(0xbac2de)
    }
    pub fn overlay0() -> Rgba {
        rgb(0x6c7086)
    }
    pub fn blue() -> Rgba {
        rgb(0x89b4fa)
    }
    pub fn green() -> Rgba {
        rgb(0xa6e3a1)
    }
    #[allow(dead_code)]
    pub fn yellow() -> Rgba {
        rgb(0xf9e2af)
    }
    pub fn peach() -> Rgba {
        rgb(0xfab387)
    }
    pub fn mauve() -> Rgba {
        rgb(0xcba6f7)
    }
    #[allow(dead_code)]
    pub fn lavender() -> Rgba {
        rgb(0xb4befe)
    }

    // Badge background colors (with alpha baked in)
    pub fn blue_bg() -> Rgba {
        rgb(0x293345)
    }
    pub fn green_bg() -> Rgba {
        rgb(0x2a3d2f)
    }
    pub fn peach_bg() -> Rgba {
        rgb(0x3d3028)
    }
    pub fn mauve_bg() -> Rgba {
        rgb(0x352e40)
    }
}

/// State of background loading operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadingState {
    Idle,
    Loading,
    Syncing,
}

/// UI interaction mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Normal navigation mode.
    Normal,
    /// Search input mode - keystrokes go to search field.
    Search,
    /// Kind filter selection mode.
    KindFilter,
    /// Source filter selection mode.
    SourceFilter,
    /// Command palette mode.
    CommandPalette,
}

/// A command available in the command palette.
#[derive(Clone)]
pub struct PaletteCommand {
    pub id: &'static str,
    pub label: &'static str,
    pub shortcut: &'static str,
}

impl PaletteCommand {
    const fn new(id: &'static str, label: &'static str, shortcut: &'static str) -> Self {
        Self {
            id,
            label,
            shortcut,
        }
    }
}

/// Available commands in the palette.
const PALETTE_COMMANDS: &[PaletteCommand] = &[
    PaletteCommand::new("search", "Search definitions", "/"),
    PaletteCommand::new("filter_kind", "Filter by kind", "f"),
    PaletteCommand::new("filter_source", "Filter by source/provider", "p"),
    PaletteCommand::new("install", "Install selected definition", "i"),
    PaletteCommand::new("sync", "Sync/refresh definitions", "s"),
    PaletteCommand::new("quit", "Quit application", "q"),
];

/// The main application state.
pub struct AppState {
    /// The data source (can be a single store or composite).
    pub source: Arc<dyn Source>,
    /// All loaded definition summaries.
    pub summaries: Vec<DefinitionSummary>,
    /// Current view summaries (filtered).
    pub view_summaries: Vec<DefinitionSummary>,
    /// Computed groups from current view.
    pub groups: Vec<Group>,
    /// Flattened rows for cursor navigation.
    pub flat_items: Vec<ListRow>,
    /// Current cursor position in flat_items.
    pub cursor: usize,
    /// Full definition for the currently selected item.
    pub selected_definition: Option<Definition>,
    /// ID of in-flight fetch to detect stale responses.
    pub pending_fetch: Option<DefinitionId>,
    /// Search query.
    pub search_query: String,
    /// Kind filter.
    pub kind_filter: Option<DefinitionKind>,
    /// Source filter.
    pub source_filter: Option<String>,
    /// Loading state.
    pub loading: LoadingState,
    /// Status message.
    pub status_message: Option<String>,
    /// Scroll offset for the list.
    pub list_scroll_offset: usize,
    /// Scroll offset for the detail pane.
    pub detail_scroll: usize,
    /// Current UI mode.
    pub mode: Mode,
    /// Cursor for filter selection (index into filter options).
    pub filter_cursor: usize,
    /// Command palette search query.
    pub palette_query: String,
    /// Command palette cursor.
    pub palette_cursor: usize,
}

impl AppState {
    pub fn new(source: Arc<dyn Source>) -> Self {
        Self {
            source,
            summaries: Vec::new(),
            view_summaries: Vec::new(),
            groups: Vec::new(),
            flat_items: Vec::new(),
            cursor: 0,
            selected_definition: None,
            pending_fetch: None,
            search_query: String::new(),
            kind_filter: None,
            source_filter: None,
            loading: LoadingState::Loading,
            status_message: Some("Loading definitions...".into()),
            list_scroll_offset: 0,
            detail_scroll: 0,
            mode: Mode::Normal,
            filter_cursor: 0,
            palette_query: String::new(),
            palette_cursor: 0,
        }
    }

    /// Load summaries from the store.
    pub fn load_summaries(&mut self, summaries: Vec<DefinitionSummary>) {
        self.summaries = summaries;
        self.recompute_view();
        self.status_message = Some(format!("Loaded {} definitions", self.summaries.len()));
        self.loading = LoadingState::Idle;
    }

    /// Recompute the filtered view and groups.
    fn recompute_view(&mut self) {
        let view: Vec<DefinitionSummary> = self
            .summaries
            .iter()
            .filter(|s| {
                if let Some(ref kind) = self.kind_filter
                    && &s.kind != kind
                {
                    return false;
                }
                if let Some(ref source) = self.source_filter
                    && &s.source_label != source
                {
                    return false;
                }
                if !self.search_query.is_empty() {
                    let q = self.search_query.to_lowercase();
                    if !s.name.to_lowercase().contains(&q)
                        && !s
                            .description
                            .as_ref()
                            .is_some_and(|d| d.to_lowercase().contains(&q))
                    {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        self.view_summaries = view;
        let (groups, flat_items) = grouping::build_groups(&self.view_summaries);
        self.groups = groups;
        self.flat_items = flat_items;
        self.cursor = grouping::first_item_index(&self.flat_items).unwrap_or(0);
        self.list_scroll_offset = 0;
    }

    /// Get the currently selected summary.
    pub fn selected_summary(&self) -> Option<&DefinitionSummary> {
        self.flat_items.get(self.cursor).and_then(|row| match row {
            ListRow::Item { summary_index } => self.view_summaries.get(*summary_index),
            ListRow::Header { .. } => None,
        })
    }

    /// Move cursor down.
    pub fn move_cursor_down(&mut self) {
        self.cursor = grouping::next_item_index(&self.flat_items, self.cursor);
    }

    /// Move cursor up.
    pub fn move_cursor_up(&mut self) {
        self.cursor = grouping::prev_item_index(&self.flat_items, self.cursor);
    }

    /// Set the selected definition.
    pub fn set_selected_definition(
        &mut self,
        id: DefinitionId,
        result: Result<Definition, String>,
    ) {
        if self.pending_fetch.as_ref() != Some(&id) {
            return; // Stale fetch
        }
        self.pending_fetch = None;
        self.loading = LoadingState::Idle;

        match result {
            Ok(def) => {
                self.selected_definition = Some(def);
                self.detail_scroll = 0;
            }
            Err(msg) => {
                self.selected_definition = None;
                self.status_message = Some(format!("Error: {msg}"));
            }
        }
    }

    /// Append a character to the search query.
    pub fn search_append(&mut self, ch: char) {
        self.search_query.push(ch);
        self.recompute_view();
    }

    /// Remove last character from search query.
    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        self.recompute_view();
    }

    /// Get all unique source labels from summaries.
    pub fn unique_sources(&self) -> Vec<String> {
        let mut sources: Vec<String> = self
            .summaries
            .iter()
            .map(|s| s.source_label.clone())
            .collect();
        sources.sort();
        sources.dedup();
        sources
    }

    /// Set kind filter.
    pub fn set_kind_filter(&mut self, kind: Option<DefinitionKind>) {
        self.kind_filter = kind;
        self.recompute_view();
    }

    /// Set source filter.
    pub fn set_source_filter(&mut self, source: Option<String>) {
        self.source_filter = source;
        self.recompute_view();
    }

    /// Clear all filters.
    pub fn clear_filters(&mut self) {
        self.search_query.clear();
        self.kind_filter = None;
        self.source_filter = None;
        self.recompute_view();
    }

    /// Get all available definition kinds (for filter options).
    pub fn available_kinds() -> Vec<Option<DefinitionKind>> {
        vec![
            None, // "All" option
            Some(DefinitionKind::Agent),
            Some(DefinitionKind::Command),
            Some(DefinitionKind::Hook),
            Some(DefinitionKind::Mcp),
            Some(DefinitionKind::Setting),
            Some(DefinitionKind::Skill),
        ]
    }

    /// Get the label for a kind filter option.
    pub fn kind_option_label(kind: &Option<DefinitionKind>) -> &'static str {
        match kind {
            None => "All Kinds",
            Some(DefinitionKind::Agent) => "Agents",
            Some(DefinitionKind::Command) => "Commands",
            Some(DefinitionKind::Hook) => "Hooks",
            Some(DefinitionKind::Mcp) => "MCP Servers",
            Some(DefinitionKind::Setting) => "Settings",
            Some(DefinitionKind::Skill) => "Skills",
            Some(DefinitionKind::Other(_)) => "Other",
        }
    }

    /// Get source filter options (None = All, Some = specific source).
    pub fn source_options(&self) -> Vec<Option<String>> {
        let mut opts = vec![None]; // "All" option
        opts.extend(self.unique_sources().into_iter().map(Some));
        opts
    }

    /// Get the label for a source filter option.
    pub fn source_option_label(source: &Option<String>) -> String {
        match source {
            None => "All Sources".to_string(),
            Some(s) => s.clone(),
        }
    }

    /// Get filtered palette commands based on query.
    pub fn filtered_palette_commands(&self) -> Vec<&'static PaletteCommand> {
        if self.palette_query.is_empty() {
            PALETTE_COMMANDS.iter().collect()
        } else {
            let query = self.palette_query.to_lowercase();
            PALETTE_COMMANDS
                .iter()
                .filter(|cmd| {
                    cmd.label.to_lowercase().contains(&query)
                        || cmd.id.to_lowercase().contains(&query)
                })
                .collect()
        }
    }
}

/// The main GPUI view.
pub struct AgentDefsApp {
    pub state: AppState,
    focus_handle: FocusHandle,
    /// List state for virtual scrolling - only renders visible items.
    list_state: ListState,
    /// Width of the list pane in pixels (resizable via drag).
    list_pane_width: f32,
    /// Whether the user is currently dragging the divider.
    is_dragging_divider: bool,
    /// Starting mouse X position when drag began (captured on first drag_move).
    drag_start_mouse_x: Option<f32>,
}

impl Focusable for AgentDefsApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl AgentDefsApp {
    pub fn new(source: Arc<dyn Source>, cx: &mut Context<Self>) -> Self {
        let state = AppState::new(Arc::clone(&source));
        let focus_handle = cx.focus_handle();
        // Initialize list state with 0 items; will be updated when data loads.
        // Overdraw of 100px ensures smooth scrolling by pre-rendering items just outside view.
        let list_state = ListState::new(0, ListAlignment::Top, px(100.0));

        // Spawn async task to load definitions
        cx.spawn(
            async move |this: WeakEntity<AgentDefsApp>, cx: &mut AsyncApp| {
                let summaries = source.list().await.unwrap_or_default();
                let _ = this.update(
                    cx,
                    |app: &mut AgentDefsApp, cx: &mut Context<AgentDefsApp>| {
                        app.state.load_summaries(summaries);
                        // Update list state with new item count
                        app.list_state.reset(app.state.flat_items.len());
                        // Fetch the first definition if available
                        if let Some(summary) = app.state.selected_summary() {
                            let id = summary.id.clone();
                            app.state.pending_fetch = Some(id.clone());
                            app.state.loading = LoadingState::Loading;

                            let source = Arc::clone(&app.state.source);
                            cx.spawn(
                                async move |this: WeakEntity<AgentDefsApp>, cx: &mut AsyncApp| {
                                    let result = source.fetch(&id).await.map_err(|e| e.to_string());
                                    let _ = this.update(
                                    cx,
                                    |app: &mut AgentDefsApp, _cx: &mut Context<AgentDefsApp>| {
                                        app.state.set_selected_definition(id, result);
                                    },
                                );
                                },
                            )
                            .detach();
                        }
                        cx.notify();
                    },
                );
            },
        )
        .detach();

        Self {
            state,
            focus_handle,
            list_state,
            list_pane_width: 300.0, // Default width
            is_dragging_divider: false,
            drag_start_mouse_x: None,
        }
    }

    /// Sync the list state with the current flat_items count.
    /// Call this after any operation that changes flat_items.
    fn sync_list_state(&self) {
        let current_count = self.list_state.item_count();
        let new_count = self.state.flat_items.len();
        if current_count != new_count {
            self.list_state.reset(new_count);
        }
    }

    pub fn fetch_current(&mut self, cx: &mut Context<Self>) {
        if let Some(summary) = self.state.selected_summary() {
            let id = summary.id.clone();

            // Don't re-fetch if already pending or already loaded
            if self.state.pending_fetch.as_ref() == Some(&id) {
                return;
            }
            if let Some(def) = &self.state.selected_definition
                && def.id == id
            {
                return;
            }

            self.state.pending_fetch = Some(id.clone());
            self.state.loading = LoadingState::Loading;

            let source = Arc::clone(&self.state.source);
            cx.spawn(
                async move |this: WeakEntity<AgentDefsApp>, cx: &mut AsyncApp| {
                    let result = source.fetch(&id).await.map_err(|e| e.to_string());
                    let _ = this.update(
                        cx,
                        |app: &mut AgentDefsApp, cx: &mut Context<AgentDefsApp>| {
                            app.state.set_selected_definition(id, result);
                            cx.notify();
                        },
                    );
                },
            )
            .detach();
        }
    }

    pub fn do_sync(&mut self, cx: &mut Context<Self>) {
        if self.state.loading == LoadingState::Idle {
            self.state.loading = LoadingState::Syncing;
            self.state.status_message = Some("Refreshing definitions from database...".into());
            cx.notify();

            let source = Arc::clone(&self.state.source);
            cx.spawn(
                async move |this: WeakEntity<AgentDefsApp>, cx: &mut AsyncApp| {
                    // Update status to show we're fetching
                    let _ = this.update(cx, |app, cx| {
                        app.state.status_message = Some("Loading definitions...".into());
                        cx.notify();
                    });

                    let summaries = source.list().await.unwrap_or_default();
                    let count = summaries.len();

                    let _ = this.update(
                        cx,
                        |app: &mut AgentDefsApp, cx: &mut Context<AgentDefsApp>| {
                            let previous_count = app.state.summaries.len();
                            app.state.load_summaries(summaries);

                            // Show informative message about what changed
                            let message = if count == previous_count {
                                format!("Refreshed: {} definitions (no changes)", count)
                            } else if count > previous_count {
                                format!(
                                    "Refreshed: {} definitions (+{} new)",
                                    count,
                                    count - previous_count
                                )
                            } else {
                                format!(
                                    "Refreshed: {} definitions (-{} removed)",
                                    count,
                                    previous_count - count
                                )
                            };
                            app.state.status_message = Some(message);
                            cx.notify();
                        },
                    );
                },
            )
            .detach();
        }
    }

    pub fn do_install(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Must have a selected definition with content
        let Some(def) = &self.state.selected_definition else {
            self.state.status_message = Some("No definition selected".into());
            cx.notify();
            return;
        };

        self.install_definition(def.clone(), cx);
    }

    /// Install a specific definition by fetching it first if needed, then prompting for directory.
    pub fn install_by_id(&mut self, id: DefinitionId, cx: &mut Context<Self>) {
        self.state.status_message = Some("Fetching definition for install...".into());
        cx.notify();

        let source = Arc::clone(&self.state.source);
        cx.spawn(
            async move |this: WeakEntity<AgentDefsApp>, cx: &mut AsyncApp| match source
                .fetch(&id)
                .await
            {
                Ok(def) => {
                    let _ = this.update(cx, |app, cx| {
                        app.install_definition(def, cx);
                    });
                }
                Err(e) => {
                    let _ = this.update(cx, |app, cx| {
                        app.state.status_message =
                            Some(format!("Failed to fetch definition: {}", e));
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    /// Install a definition - prompts for directory and writes file.
    fn install_definition(&mut self, def: Definition, cx: &mut Context<Self>) {
        if def.raw.is_empty() {
            self.state.status_message = Some("Definition has no raw content to install".into());
            cx.notify();
            return;
        }

        // Open native directory picker
        let paths_receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Select install directory".into()),
        });

        cx.spawn(
            async move |this: WeakEntity<AgentDefsApp>, cx: &mut AsyncApp| {
                // paths_receiver is Receiver<Result<Option<Vec<PathBuf>>, Error>>
                // .await gives Result<Result<...>, RecvError>
                let Ok(Ok(Some(paths))) = paths_receiver.await else {
                    return;
                };
                let Some(target_dir) = paths.first() else {
                    return;
                };

                // Install the definition
                match agent_defs::install::install_definition(target_dir, &def) {
                    Ok(installed_path) => {
                        let _ = this.update(cx, |app, cx| {
                            app.state.status_message =
                                Some(format!("Installed to {}", installed_path.display()));
                            cx.notify();
                        });
                    }
                    Err(e) => {
                        let _ = this.update(cx, |app, cx| {
                            app.state.status_message = Some(format!("Install failed: {}", e));
                            cx.notify();
                        });
                    }
                }
            },
        )
        .detach();
    }

    fn render_list_pane(&self, entity: Entity<Self>) -> impl IntoElement {
        // Clone data needed for the list render closure.
        // This allows virtual scrolling - only visible items are rendered.
        let flat_items = self.state.flat_items.clone();
        let view_summaries = self.state.view_summaries.clone();
        let cursor = self.state.cursor;
        let list_state = self.list_state.clone();
        let width = self.list_pane_width;

        // Search state for sidebar header
        let is_searching = self.state.mode == Mode::Search;
        let has_query = !self.state.search_query.is_empty();
        let search_text = if has_query {
            self.state.search_query.clone()
        } else if is_searching {
            "Type to search...".to_string()
        } else {
            "Search...".to_string()
        };
        let search_text_color = if has_query {
            colors::text()
        } else {
            colors::overlay0()
        };
        let search_border = if is_searching {
            colors::blue()
        } else {
            colors::surface1()
        };
        let is_loading = self.state.loading != LoadingState::Idle;
        let def_count = self.state.summaries.len();

        div()
            .flex()
            .flex_col()
            .flex_shrink_0() // Don't let flex override our explicit width
            .w(px(width))
            .min_w(px(150.0)) // Minimum width
            .max_w(px(600.0)) // Maximum width
            .h_full()
            .bg(colors::surface0())
            // Sidebar header with title, search, and count
            .child(
                div()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(colors::surface1())
                    // Title row
                    .child(
                        div()
                            .h(px(40.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_color(colors::text())
                                            .text_size(px(14.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("Browser"),
                                    )
                                    .when(is_loading, |el| {
                                        el.child(
                                            div()
                                                .px(px(4.0))
                                                .py(px(1.0))
                                                .bg(colors::peach_bg())
                                                .rounded(px(3.0))
                                                .child(
                                                    div()
                                                        .text_color(colors::peach())
                                                        .text_size(px(9.0))
                                                        .child(match self.state.loading {
                                                            LoadingState::Loading => "...",
                                                            LoadingState::Syncing => "sync",
                                                            LoadingState::Idle => "",
                                                        }),
                                                ),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_color(colors::overlay0())
                                    .text_size(px(11.0))
                                    .child(format!("{}", def_count)),
                            ),
                    )
                    // Search row
                    .child(
                        div().h(px(36.0)).px(px(12.0)).pb(px(8.0)).child(
                            div()
                                .w_full()
                                .h(px(28.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .bg(colors::base())
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(search_border)
                                .child(
                                    div()
                                        .text_color(colors::overlay0())
                                        .text_size(px(11.0))
                                        .mr(px(6.0))
                                        .child("/"),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .truncate()
                                        .text_color(search_text_color)
                                        .text_size(px(12.0))
                                        .child(search_text),
                                )
                                .when(is_searching, |el| {
                                    el.child(div().w(px(1.0)).h(px(12.0)).bg(colors::text()))
                                }),
                        ),
                    ),
            )
            .child(
                // Virtualized list - only renders visible items
                list(list_state, move |idx, _window, _cx| {
                    let is_selected = idx == cursor;

                    match &flat_items[idx] {
                        ListRow::Header { label, count } => div()
                            .h(px(28.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .bg(colors::base())
                            .child(
                                div()
                                    .text_color(colors::subtext0())
                                    .text_size(px(11.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(label.clone()),
                            )
                            .child(
                                div()
                                    .text_color(colors::overlay0())
                                    .text_size(px(10.0))
                                    .child(format!("{count}")),
                            )
                            .into_any_element(),
                        ListRow::Item { summary_index } => {
                            let summary = &view_summaries[*summary_index];
                            let summary_id = summary.id.clone();
                            let bg = if is_selected {
                                colors::surface1()
                            } else {
                                colors::surface0()
                            };
                            let name_color = if is_selected {
                                colors::blue()
                            } else {
                                colors::text()
                            };

                            // Clone entity for click handlers
                            let entity_for_click = entity.clone();
                            let entity_for_install = entity.clone();

                            div()
                                .id(gpui::ElementId::Integer(idx as u64))
                                .w_full()
                                .h(px(36.0))
                                .px(px(12.0))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(8.0))
                                .overflow_hidden()
                                .bg(bg)
                                .cursor_pointer()
                                .hover(|style| style.bg(colors::surface1()))
                                .on_click(move |event, _window, cx| {
                                    // Check for double-click to install
                                    let click_count = match event {
                                        gpui::ClickEvent::Mouse(mouse) => mouse.down.click_count,
                                        gpui::ClickEvent::Keyboard(_) => 1,
                                    };

                                    entity_for_click.update(cx, |app, cx| {
                                        app.state.cursor = idx;
                                        app.list_state.scroll_to_reveal_item(idx);
                                        app.fetch_current(cx);

                                        // Double-click triggers install
                                        if click_count >= 2 {
                                            if let Some(summary) = app.state.selected_summary() {
                                                app.install_by_id(summary.id.clone(), cx);
                                            }
                                        }
                                        cx.notify();
                                    });
                                })
                                // Left side: name and description (must shrink to make room for button)
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .min_w(px(0.0)) // Allow shrinking below content size
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .w_full()
                                                .truncate()
                                                .text_color(name_color)
                                                .text_size(px(13.0))
                                                .child(summary.name.clone()),
                                        )
                                        .children(summary.description.as_ref().map(|desc| {
                                            div()
                                                .w_full()
                                                .truncate()
                                                .text_color(colors::overlay0())
                                                .text_size(px(11.0))
                                                .child(desc.clone())
                                        })),
                                )
                                // Right side: install chip (fixed size, don't shrink)
                                .child(
                                    div()
                                        .id(gpui::ElementId::Name(
                                            format!("install-{}", idx).into(),
                                        ))
                                        .flex_shrink_0()
                                        .px(px(6.0))
                                        .py(px(2.0))
                                        .rounded(px(4.0))
                                        .bg(colors::green_bg())
                                        .text_color(colors::green())
                                        .text_size(px(10.0))
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(colors::green()).text_color(colors::base())
                                        })
                                        .on_click(move |_event, _window, cx| {
                                            // Stop propagation by installing directly
                                            entity_for_install.update(cx, |app, cx| {
                                                app.install_by_id(summary_id.clone(), cx);
                                            });
                                        })
                                        .child("Install"),
                                )
                                .into_any_element()
                        }
                    }
                })
                .flex_1(),
            )
    }

    fn render_resize_handle(&self, _entity: Entity<Self>) -> impl IntoElement {
        let is_dragging = self.is_dragging_divider;
        let current_width = self.list_pane_width;

        // Use a visible color - overlay0 for idle, blue when active
        let bg_color = if is_dragging {
            colors::blue()
        } else {
            colors::overlay0()
        };

        div()
            .id("resize-handle")
            // Wider hit area (8px) but only 2px visible line centered inside
            .w(px(8.0))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .child(
                // The visible line
                div().w(px(2.0)).h_full().bg(bg_color),
            )
            .hover(|style| style.bg(colors::surface1()))
            // Use on_drag to initiate drag tracking - movement handled by root's on_drag_move
            .on_drag(
                ResizeHandleDrag {
                    start_width: current_width,
                },
                |drag, _point, _window, cx| {
                    // Return an empty view for the drag visual (we don't need one for resize)
                    cx.new(|_| EmptyDragView {
                        _start_width: drag.start_width,
                    })
                },
            )
    }

    fn render_detail_pane(&self, entity: Entity<Self>) -> impl IntoElement {
        // Clone ID for install button closure
        let def_id_for_install = self
            .state
            .selected_definition
            .as_ref()
            .map(|d| d.id.clone());

        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .bg(colors::base())
            .child(
                // Detail header
                div()
                    .h(px(32.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(colors::surface1())
                    .child(
                        div()
                            .text_color(colors::subtext0())
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("DETAILS"),
                    )
                    // Install button in header (when definition is selected)
                    .when(def_id_for_install.is_some(), |el| {
                        let id = def_id_for_install.clone().unwrap();
                        let entity_for_install = entity.clone();
                        el.child(
                            div()
                                .id("detail-install-btn")
                                .px(px(12.0))
                                .py(px(4.0))
                                .rounded(px(4.0))
                                .bg(colors::green_bg())
                                .text_color(colors::green())
                                .text_size(px(11.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .hover(|style| style.bg(colors::green()).text_color(colors::base()))
                                .on_click(move |_event, _window, cx| {
                                    entity_for_install.update(cx, |app, cx| {
                                        app.install_by_id(id.clone(), cx);
                                    });
                                })
                                .child("Install"),
                        )
                    }),
            )
            .child(
                // Detail content - using id() to enable InteractiveElement trait
                div()
                    .id("detail-content")
                    .flex_1()
                    .min_w(px(0.0)) // Allow shrinking
                    .p(px(16.0))
                    .overflow_y_scroll()
                    .overflow_x_hidden()
                    .children(self.state.selected_definition.as_ref().map(|def| {
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .min_w(px(0.0)) // Allow shrinking
                            .gap(px(12.0))
                            // Name
                            .child(
                                div()
                                    .w_full()
                                    .text_color(colors::text())
                                    .text_size(px(18.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(def.name.clone()),
                            )
                            // Metadata row
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(12.0))
                                    .child(render_badge(
                                        grouping::kind_label(&def.kind),
                                        colors::blue(),
                                        colors::blue_bg(),
                                    ))
                                    .child(render_badge(
                                        &def.source_label,
                                        colors::green(),
                                        colors::green_bg(),
                                    )),
                            )
                            // Description
                            .children(def.description.as_ref().map(|desc| {
                                div()
                                    .w_full()
                                    .text_color(colors::subtext1())
                                    .text_size(px(13.0))
                                    .child(desc.clone())
                            }))
                            // Tools
                            .when(!def.tools.is_empty(), |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(
                                            div()
                                                .text_color(colors::subtext0())
                                                .text_size(px(11.0))
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .child("TOOLS"),
                                        )
                                        .child(div().flex().flex_wrap().gap(px(6.0)).children(
                                            def.tools.iter().map(|tool| {
                                                render_badge(
                                                    tool,
                                                    colors::peach(),
                                                    colors::peach_bg(),
                                                )
                                            }),
                                        )),
                                )
                            })
                            // Model
                            .children(def.model.as_ref().map(|model| {
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_color(colors::subtext0())
                                            .text_size(px(11.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("MODEL"),
                                    )
                                    .child(render_badge(model, colors::mauve(), colors::mauve_bg()))
                            }))
                            // Body
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_color(colors::subtext0())
                                            .text_size(px(11.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("BODY"),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .p(px(12.0))
                                            .bg(colors::surface0())
                                            .rounded(px(6.0))
                                            .overflow_hidden()
                                            .child(
                                                div()
                                                    .w_full()
                                                    .text_color(colors::text())
                                                    .text_size(px(12.0))
                                                    .child(def.body.clone()),
                                            ),
                                    ),
                            )
                    }))
                    .when(
                        self.state.selected_definition.is_none(),
                        |this: gpui::Stateful<gpui::Div>| {
                            this.child(
                                div().flex().items_center().justify_center().h_full().child(
                                    div()
                                        .text_color(colors::overlay0())
                                        .text_size(px(14.0))
                                        .child(if self.state.loading == LoadingState::Loading {
                                            "Loading..."
                                        } else {
                                            "Select a definition to view details"
                                        }),
                                ),
                            )
                        },
                    ),
            )
    }

    fn render_status_bar(&self) -> impl IntoElement {
        let status = self.state.status_message.as_deref().unwrap_or("Ready");

        let key_hints = match self.state.mode {
            Mode::Normal => {
                "j/k: navigate | /: search | f: kind | p: source | i: install | s: sync | ⌘K: commands"
            }
            Mode::Search => "type to filter | enter: confirm | esc: cancel",
            Mode::KindFilter | Mode::SourceFilter => "j/k: navigate | enter: select | esc: cancel",
            Mode::CommandPalette => "↑↓: navigate | enter: select | esc: close",
        };

        let mode_indicator = match self.state.mode {
            Mode::Normal => None,
            Mode::Search => Some("SEARCH"),
            Mode::KindFilter => Some("KIND FILTER"),
            Mode::SourceFilter => Some("SOURCE FILTER"),
            Mode::CommandPalette => Some("COMMANDS"),
        };

        div()
            .h(px(24.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .bg(colors::surface0())
            .border_t_1()
            .border_color(colors::surface1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(mode_indicator.map(|mode| {
                        div()
                            .px(px(6.0))
                            .py(px(1.0))
                            .bg(colors::blue_bg())
                            .rounded(px(3.0))
                            .child(
                                div()
                                    .text_color(colors::blue())
                                    .text_size(px(10.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(mode),
                            )
                    }))
                    .child(
                        div()
                            .text_color(colors::subtext0())
                            .text_size(px(11.0))
                            .child(status.to_string()),
                    ),
            )
            .child(
                div()
                    .text_color(colors::overlay0())
                    .text_size(px(11.0))
                    .child(key_hints),
            )
    }
}

fn render_badge(text: &str, color: gpui::Rgba, bg_color: gpui::Rgba) -> impl IntoElement {
    div()
        .px(px(8.0))
        .py(px(2.0))
        .bg(bg_color)
        .rounded(px(4.0))
        .child(
            div()
                .text_color(color)
                .text_size(px(11.0))
                .child(text.to_string()),
        )
}

impl AgentDefsApp {
    fn render_kind_filter_overlay(&self) -> impl IntoElement {
        let kinds = AppState::available_kinds();

        div()
            .absolute()
            .top(px(120.0)) // Below title + search bar
            .left(px(50.0))
            .w(px(200.0))
            .bg(colors::surface0())
            .border_1()
            .border_color(colors::surface1())
            .rounded(px(8.0))
            .shadow_lg()
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_color(colors::subtext0())
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .pb(px(4.0))
                    .child("Filter by Kind"),
            )
            .children(kinds.iter().enumerate().map(|(idx, kind)| {
                let is_selected = idx == self.state.filter_cursor;
                let label = AppState::kind_option_label(kind);
                let bg = if is_selected {
                    colors::surface1()
                } else {
                    colors::surface0()
                };
                let text_color = if is_selected {
                    colors::blue()
                } else {
                    colors::text()
                };

                div()
                    .id(gpui::ElementId::Name(format!("kind-{idx}").into()))
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .bg(bg)
                    .rounded(px(4.0))
                    .child(
                        div()
                            .text_color(text_color)
                            .text_size(px(13.0))
                            .child(label),
                    )
            }))
            .child(
                div()
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(colors::surface1())
                    .mt(px(4.0))
                    .child(
                        div()
                            .text_color(colors::overlay0())
                            .text_size(px(10.0))
                            .child("j/k: navigate | enter: select | esc: cancel"),
                    ),
            )
    }

    fn render_source_filter_overlay(&self) -> impl IntoElement {
        let sources = self.state.source_options();

        div()
            .absolute()
            .top(px(120.0)) // Below title + search bar
            .left(px(50.0))
            .w(px(250.0))
            .bg(colors::surface0())
            .border_1()
            .border_color(colors::surface1())
            .rounded(px(8.0))
            .shadow_lg()
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_color(colors::subtext0())
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .pb(px(4.0))
                    .child("Filter by Source"),
            )
            .children(sources.iter().enumerate().map(|(idx, source)| {
                let is_selected = idx == self.state.filter_cursor;
                let label = AppState::source_option_label(source);
                let bg = if is_selected {
                    colors::surface1()
                } else {
                    colors::surface0()
                };
                let text_color = if is_selected {
                    colors::green()
                } else {
                    colors::text()
                };

                div()
                    .id(gpui::ElementId::Name(format!("source-{idx}").into()))
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .bg(bg)
                    .rounded(px(4.0))
                    .child(
                        div()
                            .text_color(text_color)
                            .text_size(px(13.0))
                            .child(label),
                    )
            }))
            .child(
                div()
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(colors::surface1())
                    .mt(px(4.0))
                    .child(
                        div()
                            .text_color(colors::overlay0())
                            .text_size(px(10.0))
                            .child("j/k: navigate | enter: select | esc: cancel"),
                    ),
            )
    }

    fn render_command_palette(&self, entity: Entity<Self>) -> impl IntoElement {
        let commands = self.state.filtered_palette_commands();
        let query = self.state.palette_query.clone();

        // Centered modal overlay
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(100.0)) // Some top padding
            .bg(gpui::rgba(0x00000088)) // Semi-transparent backdrop
            .child(
                div()
                    .w(px(400.0))
                    .max_h(px(400.0))
                    .bg(colors::surface0())
                    .border_1()
                    .border_color(colors::surface1())
                    .rounded(px(12.0))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Search input area
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(12.0))
                            .border_b_1()
                            .border_color(colors::surface1())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_color(colors::overlay0())
                                            .text_size(px(14.0))
                                            .child(">"),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_color(if query.is_empty() {
                                                colors::overlay0()
                                            } else {
                                                colors::text()
                                            })
                                            .text_size(px(14.0))
                                            .child(if query.is_empty() {
                                                "Type a command...".to_string()
                                            } else {
                                                query
                                            }),
                                    ),
                            ),
                    )
                    // Commands list
                    .child(
                        div()
                            .id("palette-list")
                            .flex_1()
                            .overflow_y_scroll()
                            .p(px(8.0))
                            .children(commands.iter().enumerate().map(|(idx, cmd)| {
                                let is_selected = idx == self.state.palette_cursor;
                                let bg = if is_selected {
                                    colors::surface1()
                                } else {
                                    colors::surface0()
                                };
                                let text_color = if is_selected {
                                    colors::blue()
                                } else {
                                    colors::text()
                                };

                                let cmd_id = cmd.id;
                                let entity_for_click = entity.clone();

                                div()
                                    .id(gpui::ElementId::Name(format!("palette-{idx}").into()))
                                    .h(px(36.0))
                                    .px(px(12.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .bg(bg)
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(colors::surface1()))
                                    .on_click(move |_event, window, cx| {
                                        entity_for_click.update(cx, |app, cx| {
                                            app.execute_palette_command(cmd_id, window, cx);
                                        });
                                    })
                                    .child(
                                        div()
                                            .text_color(text_color)
                                            .text_size(px(14.0))
                                            .child(cmd.label),
                                    )
                                    .child(
                                        div()
                                            .px(px(6.0))
                                            .py(px(2.0))
                                            .bg(colors::base())
                                            .rounded(px(4.0))
                                            .text_color(colors::overlay0())
                                            .text_size(px(11.0))
                                            .child(cmd.shortcut),
                                    )
                            })),
                    )
                    // Footer hints
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(8.0))
                            .border_t_1()
                            .border_color(colors::surface1())
                            .child(
                                div()
                                    .text_color(colors::overlay0())
                                    .text_size(px(10.0))
                                    .child("↑↓: navigate | enter: select | esc: close"),
                            ),
                    ),
            )
    }

    /// Execute a command from the palette by its ID.
    fn execute_palette_command(
        &mut self,
        cmd_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Close palette first
        self.state.mode = Mode::Normal;
        self.state.palette_query.clear();
        self.state.palette_cursor = 0;

        // Execute the command
        match cmd_id {
            "search" => {
                self.state.mode = Mode::Search;
            }
            "filter_kind" => {
                self.state.mode = Mode::KindFilter;
                let kinds = AppState::available_kinds();
                self.state.filter_cursor = kinds
                    .iter()
                    .position(|k| k == &self.state.kind_filter)
                    .unwrap_or(0);
            }
            "filter_source" => {
                self.state.mode = Mode::SourceFilter;
                let sources = self.state.source_options();
                self.state.filter_cursor = sources
                    .iter()
                    .position(|s| s == &self.state.source_filter)
                    .unwrap_or(0);
            }
            "install" => {
                self.do_install(window, cx);
            }
            "sync" => {
                self.do_sync(cx);
            }
            "quit" => {
                cx.quit();
            }
            _ => {}
        }

        cx.notify();
    }
}

impl Render for AgentDefsApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Ensure we have focus for keybindings to work
        if !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window);
        }

        // Get entity handle for passing to child components that need to update state
        let entity = cx.entity().clone();

        // Create action listeners using the `listener` helper
        let on_move_down = cx.listener(|this: &mut Self, _: &MoveDown, _window, cx| {
            match this.state.mode {
                Mode::Normal => {
                    this.state.move_cursor_down();
                    this.list_state.scroll_to_reveal_item(this.state.cursor);
                    this.fetch_current(cx);
                }
                Mode::KindFilter => {
                    let max = AppState::available_kinds().len();
                    if this.state.filter_cursor + 1 < max {
                        this.state.filter_cursor += 1;
                    }
                }
                Mode::SourceFilter => {
                    let max = this.state.source_options().len();
                    if this.state.filter_cursor + 1 < max {
                        this.state.filter_cursor += 1;
                    }
                }
                Mode::Search => {
                    // 'j' key in search mode - type it
                    this.state.search_append('j');
                    this.sync_list_state();
                }
                Mode::CommandPalette => {
                    let max = this.state.filtered_palette_commands().len();
                    if this.state.palette_cursor + 1 < max {
                        this.state.palette_cursor += 1;
                    }
                }
            }
            cx.notify();
        });

        let on_move_up = cx.listener(|this: &mut Self, _: &MoveUp, _window, cx| {
            match this.state.mode {
                Mode::Normal => {
                    this.state.move_cursor_up();
                    this.list_state.scroll_to_reveal_item(this.state.cursor);
                    this.fetch_current(cx);
                }
                Mode::KindFilter | Mode::SourceFilter => {
                    if this.state.filter_cursor > 0 {
                        this.state.filter_cursor -= 1;
                    }
                }
                Mode::Search => {
                    // 'k' key in search mode - type it
                    this.state.search_append('k');
                    this.sync_list_state();
                }
                Mode::CommandPalette => {
                    if this.state.palette_cursor > 0 {
                        this.state.palette_cursor -= 1;
                    }
                }
            }
            cx.notify();
        });

        let on_sync = cx.listener(|this: &mut Self, _: &SyncAction, _window, cx| {
            if this.state.mode == Mode::Normal {
                this.do_sync(cx);
            } else if this.state.mode == Mode::Search {
                this.state.search_append('s');
                this.sync_list_state();
                cx.notify();
            } else if this.state.mode == Mode::CommandPalette {
                this.state.palette_query.push('s');
                this.state.palette_cursor = 0;
                cx.notify();
            }
        });

        let on_enter_search = cx.listener(|this: &mut Self, _: &EnterSearch, _window, cx| {
            if this.state.mode == Mode::Normal {
                this.state.mode = Mode::Search;
                cx.notify();
            }
        });

        let on_exit_search = cx.listener(|this: &mut Self, _: &ExitSearch, _window, cx| {
            // Reset to normal mode from any overlay
            this.state.mode = Mode::Normal;
            this.state.filter_cursor = 0;
            this.state.palette_query.clear();
            this.state.palette_cursor = 0;
            cx.notify();
        });

        let on_clear_filters = cx.listener(|this: &mut Self, _: &ClearFilters, _window, cx| {
            if this.state.mode == Mode::Search {
                // In search mode, backspace removes a character
                this.state.search_backspace();
                this.sync_list_state();
            } else if this.state.mode == Mode::CommandPalette {
                // In palette mode, backspace removes from palette query
                this.state.palette_query.pop();
                // Reset cursor if filtered list changed
                let max = this.state.filtered_palette_commands().len();
                if this.state.palette_cursor >= max {
                    this.state.palette_cursor = max.saturating_sub(1);
                }
            } else if this.state.mode == Mode::Normal {
                // In normal mode, clear all filters
                this.state.clear_filters();
                this.sync_list_state();
            }
            cx.notify();
        });

        let on_select_item = cx.listener(|this: &mut Self, _: &SelectItem, window, cx| {
            match this.state.mode {
                Mode::Search => {
                    // Exit search mode on enter
                    this.state.mode = Mode::Normal;
                }
                Mode::KindFilter => {
                    // Apply the selected kind filter
                    let kinds = AppState::available_kinds();
                    if let Some(kind) = kinds.get(this.state.filter_cursor) {
                        this.state.set_kind_filter(kind.clone());
                        this.sync_list_state();
                    }
                    this.state.mode = Mode::Normal;
                    this.state.filter_cursor = 0;
                }
                Mode::SourceFilter => {
                    // Apply the selected source filter
                    let sources = this.state.source_options();
                    if let Some(source) = sources.get(this.state.filter_cursor) {
                        this.state.set_source_filter(source.clone());
                        this.sync_list_state();
                    }
                    this.state.mode = Mode::Normal;
                    this.state.filter_cursor = 0;
                }
                Mode::CommandPalette => {
                    // Execute the selected command
                    let commands = this.state.filtered_palette_commands();
                    if let Some(cmd) = commands.get(this.state.palette_cursor) {
                        let cmd_id = cmd.id;
                        this.execute_palette_command(cmd_id, window, cx);
                    }
                }
                Mode::Normal => {}
            }
            cx.notify();
        });

        let on_enter_kind_filter =
            cx.listener(|this: &mut Self, _: &EnterKindFilter, _window, cx| {
                if this.state.mode == Mode::Normal {
                    this.state.mode = Mode::KindFilter;
                    // Set cursor to current selection if any
                    let kinds = AppState::available_kinds();
                    this.state.filter_cursor = kinds
                        .iter()
                        .position(|k| k == &this.state.kind_filter)
                        .unwrap_or(0);
                    cx.notify();
                } else if this.state.mode == Mode::Search {
                    // 'f' key in search mode - type it
                    this.state.search_append('f');
                    this.sync_list_state();
                    cx.notify();
                } else if this.state.mode == Mode::CommandPalette {
                    // 'f' key in palette mode - type it
                    this.state.palette_query.push('f');
                    this.state.palette_cursor = 0;
                    cx.notify();
                }
            });

        let on_enter_source_filter =
            cx.listener(|this: &mut Self, _: &EnterSourceFilter, _window, cx| {
                if this.state.mode == Mode::Normal {
                    this.state.mode = Mode::SourceFilter;
                    // Set cursor to current selection if any
                    let sources = this.state.source_options();
                    this.state.filter_cursor = sources
                        .iter()
                        .position(|s| s == &this.state.source_filter)
                        .unwrap_or(0);
                    cx.notify();
                } else if this.state.mode == Mode::Search {
                    // 'p' key in search mode - type it
                    this.state.search_append('p');
                    this.sync_list_state();
                    cx.notify();
                } else if this.state.mode == Mode::CommandPalette {
                    // 'p' key in palette mode - type it
                    this.state.palette_query.push('p');
                    this.state.palette_cursor = 0;
                    cx.notify();
                }
            });

        let on_install = cx.listener(|this: &mut Self, _: &Install, window, cx| {
            if this.state.mode == Mode::Normal {
                this.do_install(window, cx);
            } else if this.state.mode == Mode::Search {
                // 'i' key in search mode - type it
                this.state.search_append('i');
                this.sync_list_state();
                cx.notify();
            } else if this.state.mode == Mode::CommandPalette {
                // 'i' key in palette mode - type it
                this.state.palette_query.push('i');
                this.state.palette_cursor = 0;
                cx.notify();
            }
        });

        let on_quit = cx.listener(|this: &mut Self, _: &Quit, _window, cx| {
            if this.state.mode == Mode::Search {
                // 'q' key in search mode - type it instead of quitting
                this.state.search_append('q');
                this.sync_list_state();
                cx.notify();
            } else if this.state.mode == Mode::CommandPalette {
                // 'q' key in palette mode - type it instead of quitting
                this.state.palette_query.push('q');
                this.state.palette_cursor = 0;
                cx.notify();
            } else {
                // Normal mode - quit the app
                cx.quit();
            }
        });

        let on_toggle_palette =
            cx.listener(|this: &mut Self, _: &ToggleCommandPalette, _window, cx| {
                if this.state.mode == Mode::CommandPalette {
                    // Close palette
                    this.state.mode = Mode::Normal;
                    this.state.palette_query.clear();
                    this.state.palette_cursor = 0;
                } else {
                    // Open palette
                    this.state.mode = Mode::CommandPalette;
                    this.state.palette_query.clear();
                    this.state.palette_cursor = 0;
                }
                cx.notify();
            });

        // Handle key input for search mode and command palette
        let on_key_down =
            cx.listener(|this: &mut Self, event: &gpui::KeyDownEvent, _window, cx| {
                if let Some(key_char) = &event.keystroke.key_char
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                    && !event.keystroke.modifiers.platform
                {
                    match this.state.mode {
                        Mode::Search => {
                            for ch in key_char.chars() {
                                if !ch.is_control() {
                                    this.state.search_append(ch);
                                }
                            }
                            this.sync_list_state();
                            cx.notify();
                        }
                        Mode::CommandPalette => {
                            for ch in key_char.chars() {
                                if !ch.is_control() {
                                    this.state.palette_query.push(ch);
                                }
                            }
                            // Reset cursor when filter changes
                            this.state.palette_cursor = 0;
                            cx.notify();
                        }
                        _ => {}
                    }
                }
            });

        // Determine if we should show overlays
        let show_kind_filter = self.state.mode == Mode::KindFilter;
        let show_source_filter = self.state.mode == Mode::SourceFilter;
        let show_command_palette = self.state.mode == Mode::CommandPalette;

        // Show resize cursor when dragging
        let is_dragging = self.is_dragging_divider;
        let entity_for_drop = entity.clone();
        let entity_for_drag_move = entity.clone();

        div()
            .id("root")
            .key_context("AgentDefsApp")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(colors::base())
            .when(is_dragging, |el| el.cursor(CursorStyle::ResizeLeftRight))
            // Handle drag move events for resize - must be on root to receive events across whole window
            .on_drag_move(
                move |event: &gpui::DragMoveEvent<ResizeHandleDrag>, _window, cx| {
                    let start_width = event.drag(cx).start_width;
                    let current_x: f32 = event.event.position.x.into();

                    entity_for_drag_move.update(cx, |app, cx| {
                        // On first drag move, capture the starting mouse position
                        if app.drag_start_mouse_x.is_none() {
                            app.drag_start_mouse_x = Some(current_x);
                        }

                        // Calculate delta from where drag started
                        let start_x = app.drag_start_mouse_x.unwrap_or(current_x);
                        let delta = current_x - start_x;
                        let new_width = (start_width + delta).clamp(150.0, 600.0);
                        app.list_pane_width = new_width;
                        app.is_dragging_divider = true;
                        cx.notify();
                    });
                },
            )
            // Handle drop of resize drag to clean up state
            .on_drop(move |_drag: &ResizeHandleDrag, _window, cx| {
                entity_for_drop.update(cx, |app, cx| {
                    app.is_dragging_divider = false;
                    app.drag_start_mouse_x = None;
                    cx.notify();
                });
            })
            .on_action(on_quit)
            .on_action(on_move_down)
            .on_action(on_move_up)
            .on_action(on_sync)
            .on_action(on_enter_search)
            .on_action(on_exit_search)
            .on_action(on_clear_filters)
            .on_action(on_select_item)
            .on_action(on_enter_kind_filter)
            .on_action(on_enter_source_filter)
            .on_action(on_install)
            .on_action(on_toggle_palette)
            .on_key_down(on_key_down)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(self.render_list_pane(entity.clone()))
                    .child(self.render_resize_handle(entity.clone()))
                    .child(self.render_detail_pane(entity.clone())),
            )
            .child(self.render_status_bar())
            // Filter overlays
            .when(show_kind_filter, |el| {
                el.child(self.render_kind_filter_overlay())
            })
            .when(show_source_filter, |el| {
                el.child(self.render_source_filter_overlay())
            })
            // Command palette overlay
            .when(show_command_palette, |el| {
                el.child(self.render_command_palette(entity))
            })
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    /// Test the resize calculation logic in isolation.
    /// This mirrors what happens in on_drag_move.
    fn calculate_new_width(
        start_width: f32,
        start_mouse_x: Option<f32>,
        current_x: f32,
    ) -> (f32, Option<f32>) {
        // On first drag move, capture the starting mouse position
        let start_x = start_mouse_x.unwrap_or(current_x);
        let new_start_mouse_x = Some(start_x);

        // Calculate delta from where drag started
        let delta = current_x - start_x;
        let new_width = (start_width + delta).clamp(150.0, 600.0);

        (new_width, new_start_mouse_x)
    }

    #[test]
    fn resize_first_move_captures_start_position() {
        // First move: start_mouse_x is None, should capture current position
        let (new_width, new_start_x) = calculate_new_width(300.0, None, 350.0);

        // First move, delta is 0, width unchanged
        assert_eq!(new_width, 300.0);
        assert_eq!(new_start_x, Some(350.0));
    }

    #[test]
    fn resize_drag_right_increases_width() {
        // Start at mouse X=300, panel width=300
        // First move captures position
        let (_, start_x) = calculate_new_width(300.0, None, 300.0);

        // Drag right to X=350 (delta = +50)
        let (new_width, _) = calculate_new_width(300.0, start_x, 350.0);
        assert_eq!(new_width, 350.0);
    }

    #[test]
    fn resize_drag_left_decreases_width() {
        // Start at mouse X=300, panel width=300
        let (_, start_x) = calculate_new_width(300.0, None, 300.0);

        // Drag left to X=250 (delta = -50)
        let (new_width, _) = calculate_new_width(300.0, start_x, 250.0);
        assert_eq!(new_width, 250.0);
    }

    #[test]
    fn resize_respects_minimum_width() {
        // Start at mouse X=300, panel width=300
        let (_, start_x) = calculate_new_width(300.0, None, 300.0);

        // Drag far left (delta = -200, would be 100 but clamped to 150)
        let (new_width, _) = calculate_new_width(300.0, start_x, 100.0);
        assert_eq!(new_width, 150.0);
    }

    #[test]
    fn resize_respects_maximum_width() {
        // Start at mouse X=300, panel width=300
        let (_, start_x) = calculate_new_width(300.0, None, 300.0);

        // Drag far right (delta = +400, would be 700 but clamped to 600)
        let (new_width, _) = calculate_new_width(300.0, start_x, 700.0);
        assert_eq!(new_width, 600.0);
    }

    #[test]
    fn resize_uses_start_width_from_drag_data() {
        // Panel is at 400px, user starts dragging
        let (_, start_x) = calculate_new_width(400.0, None, 400.0);

        // Drag right by 50px
        let (new_width, _) = calculate_new_width(400.0, start_x, 450.0);
        assert_eq!(new_width, 450.0);

        // Continue dragging right by another 50px (total delta = 100)
        let (new_width, _) = calculate_new_width(400.0, start_x, 500.0);
        assert_eq!(new_width, 500.0);
    }
}
