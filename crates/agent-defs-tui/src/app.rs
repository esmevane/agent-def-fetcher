use std::path::PathBuf;
use std::time::Instant;

use agent_defs::{Definition, DefinitionId, DefinitionKind, DefinitionSummary};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui_explorer::{FileExplorer, Input, Theme};

/// Duration threshold for detecting double-clicks (in milliseconds).
const DOUBLE_CLICK_THRESHOLD_MS: u128 = 400;

use crate::action::{Action, AppCommand};
use crate::grouping::{self, Group, ListRow};
use crate::SyncResult;

/// Tracks clickable regions for mouse hit testing.
#[derive(Debug, Clone, Default)]
pub struct LayoutGeometry {
    /// Inner area of the list pane (excluding borders).
    pub list_inner: Rect,
    /// Inner area of the detail pane (excluding borders).
    pub detail_inner: Rect,
    /// Overlay area if one is currently displayed.
    pub overlay: Option<Rect>,
    /// Inner area of the file explorer list (for click-to-select in InstallPrompt mode).
    pub explorer_list_inner: Option<Rect>,
}

/// UI mode the app is currently in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    KindFilter,
    SourceFilter,
    InstallPrompt,
    InstallConfirm,
    SyncProgress,
}

/// State of background loading operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadingState {
    Idle,
    Fetching,
    Syncing,
}

/// Transient status message shown in the status bar.
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
    /// Remaining ticks before the message expires.
    pub ticks_remaining: u8,
}

/// The TUI application state. This is a pure state machine:
/// inputs produce commands (side effects), actions update state.
pub struct App {
    /// All loaded definition summaries (unfiltered).
    pub summaries: Vec<DefinitionSummary>,
    /// Current view summaries (filtered by search or same as summaries).
    pub view_summaries: Vec<DefinitionSummary>,
    /// Source label for display.
    pub source_label: String,
    /// Computed groups from current view.
    pub groups: Vec<Group>,
    /// Flattened rows for cursor navigation.
    pub flat_items: Vec<ListRow>,
    /// Current cursor position in flat_items.
    pub cursor: usize,
    /// Viewport scroll offset for list pane.
    pub list_scroll_offset: usize,

    /// Full definition for the currently selected item.
    pub selected_definition: Option<Definition>,
    /// Detail pane body scroll offset.
    pub detail_scroll: u16,
    /// ID of in-flight fetch to detect stale responses.
    pub pending_fetch: Option<DefinitionId>,

    /// Current UI mode.
    pub mode: Mode,
    /// Active search query.
    pub search_query: String,

    /// Transient feedback message.
    pub status_message: Option<StatusMessage>,
    /// Background loading state.
    pub loading: LoadingState,

    /// Active kind filter (None = show all).
    pub kind_filter: Option<DefinitionKind>,
    /// Cursor position in the kind filter overlay list.
    pub kind_filter_cursor: usize,

    /// Active source filter (None = show all).
    pub source_filter: Option<String>,
    /// Cursor position in the source filter overlay list.
    pub source_filter_cursor: usize,

    /// Target directory for installing definitions.
    pub install_target: Option<PathBuf>,
    /// File explorer for selecting install directory.
    pub file_explorer: Option<FileExplorer>,
    /// Pending install path for confirmation dialog.
    pub pending_install_path: Option<PathBuf>,

    /// Result of last sync operation (for display in overlay).
    pub sync_result: Option<SyncResult>,
    /// Scroll offset in sync result warnings list.
    pub sync_result_scroll: usize,

    /// Layout geometry for mouse hit testing.
    pub layout_geometry: LayoutGeometry,

    /// Timestamp of last mouse click for double-click detection.
    last_click_time: Option<Instant>,
    /// Position of last mouse click for double-click detection.
    last_click_pos: Option<(u16, u16)>,
}

impl App {
    /// Create a new App from the initial set of definition summaries.
    pub fn new(summaries: Vec<DefinitionSummary>, source_label: String) -> Self {
        Self::with_install_target(summaries, source_label, None)
    }

    /// Create a new App with an optional install target directory.
    pub fn with_install_target(
        summaries: Vec<DefinitionSummary>,
        source_label: String,
        install_target: Option<PathBuf>,
    ) -> Self {
        let view_summaries = summaries.clone();
        let (groups, flat_items) = grouping::build_groups(&view_summaries);
        let cursor = grouping::first_item_index(&flat_items).unwrap_or(0);

        let mut app = Self {
            summaries,
            view_summaries,
            source_label,
            groups,
            flat_items,
            cursor,
            list_scroll_offset: 0,
            selected_definition: None,
            detail_scroll: 0,
            pending_fetch: None,
            mode: Mode::Normal,
            search_query: String::new(),
            status_message: None,
            loading: LoadingState::Idle,
            kind_filter: None,
            kind_filter_cursor: 0,
            source_filter: None,
            source_filter_cursor: 0,
            install_target,
            file_explorer: None,
            pending_install_path: None,
            sync_result: None,
            sync_result_scroll: 0,
            layout_geometry: LayoutGeometry::default(),
            last_click_time: None,
            last_click_pos: None,
        };

        // If we have items, kick off an initial fetch.
        app.maybe_fetch_current();
        app
    }

    /// Handle a terminal event, returning a command for the event loop.
    pub fn handle_event(&mut self, event: Event) -> AppCommand {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => AppCommand::None,
        }
    }

    /// Handle an async action (result from a background task).
    pub fn handle_action(&mut self, action: Action) -> AppCommand {
        match action {
            Action::DefinitionLoaded(id, result) => {
                // Discard stale fetches.
                if self.pending_fetch.as_ref() != Some(&id) {
                    return AppCommand::None;
                }
                self.pending_fetch = None;
                self.loading = LoadingState::Idle;

                match *result {
                    Ok(def) => {
                        self.selected_definition = Some(def);
                        self.detail_scroll = 0;
                    }
                    Err(msg) => {
                        self.selected_definition = None;
                        self.set_status(msg, true);
                    }
                }
                AppCommand::None
            }
            Action::ListReloaded(result) => {
                match result {
                    Ok(summaries) => {
                        self.reload(summaries);
                        self.set_status("List reloaded".into(), false);
                    }
                    Err(msg) => {
                        self.set_status(format!("Reload failed: {msg}"), true);
                    }
                }
                AppCommand::None
            }
            Action::SyncCompleted(result) => {
                self.loading = LoadingState::Idle;
                match result {
                    Ok(sync_result) => {
                        self.sync_result = Some(sync_result);
                        // Stay in SyncProgress mode to show results
                        return AppCommand::ReloadList;
                    }
                    Err(msg) => {
                        self.mode = Mode::Normal;
                        self.set_status(format!("Sync failed: {msg}"), true);
                    }
                }
                AppCommand::None
            }
            Action::CopyCompleted(result) => {
                match result {
                    Ok(()) => self.set_status("Copied to clipboard".into(), false),
                    Err(msg) => self.set_status(format!("Copy failed: {msg}"), true),
                }
                AppCommand::None
            }
            Action::InstallCompleted(result) => {
                match result {
                    Ok(msg) => self.set_status(msg, false),
                    Err(msg) => self.set_status(format!("Install failed: {msg}"), true),
                }
                AppCommand::None
            }
        }
    }

    /// Tick the app forward (called on interval). Used for expiring status messages.
    pub fn tick(&mut self) {
        if let Some(msg) = &mut self.status_message {
            if msg.ticks_remaining == 0 {
                self.status_message = None;
            } else {
                msg.ticks_remaining -= 1;
            }
        }
    }

    /// Reload the summaries list (e.g., after sync). Preserves search filter if active.
    pub fn reload(&mut self, summaries: Vec<DefinitionSummary>) {
        self.summaries = summaries;
        self.recompute_view();
    }

    /// Get the summary index for the currently selected cursor position.
    pub fn selected_summary_index(&self) -> Option<usize> {
        self.flat_items.get(self.cursor).and_then(|row| match row {
            ListRow::Item { summary_index } => Some(*summary_index),
            ListRow::Header { .. } => None,
        })
    }

    /// Get the currently selected summary.
    pub fn selected_summary(&self) -> Option<&DefinitionSummary> {
        self.selected_summary_index()
            .and_then(|idx| self.view_summaries.get(idx))
    }

    fn handle_key(&mut self, key: KeyEvent) -> AppCommand {
        // Ctrl+C always quits.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return AppCommand::Quit;
        }

        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Search => self.handle_search_key(key),
            Mode::KindFilter => self.handle_kind_filter_key(key),
            Mode::SourceFilter => self.handle_source_filter_key(key),
            Mode::InstallPrompt => self.handle_install_prompt_key(key),
            Mode::InstallConfirm => self.handle_install_confirm_key(key),
            Mode::SyncProgress => self.handle_sync_progress_key(key),
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> AppCommand {
        match self.mode {
            Mode::Normal | Mode::Search => self.handle_normal_mouse(mouse),
            Mode::KindFilter => self.handle_kind_filter_mouse(mouse),
            Mode::SourceFilter => self.handle_source_filter_mouse(mouse),
            Mode::SyncProgress => self.handle_sync_progress_mouse(mouse),
            Mode::InstallPrompt => self.handle_install_prompt_mouse(mouse),
            Mode::InstallConfirm => self.handle_install_confirm_mouse(mouse),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Char('q') => AppCommand::Quit,
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_cursor_down();
                self.maybe_fetch_current()
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_cursor_up();
                self.maybe_fetch_current()
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_detail_down();
                AppCommand::None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_detail_up();
                AppCommand::None
            }
            KeyCode::PageDown => {
                self.scroll_detail_down();
                AppCommand::None
            }
            KeyCode::PageUp => {
                self.scroll_detail_up();
                AppCommand::None
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search_query.clear();
                AppCommand::None
            }
            KeyCode::Char('f') => {
                self.mode = Mode::KindFilter;
                self.kind_filter_cursor = 0;
                AppCommand::None
            }
            KeyCode::Char('p') => {
                self.mode = Mode::SourceFilter;
                self.source_filter_cursor = 0;
                AppCommand::None
            }
            KeyCode::Enter | KeyCode::Char('i') => {
                // Enter on a header row sets kind filter to that group's kind.
                // Enter on an item row starts the installer.
                if let Some(kind) = self.header_kind_at_cursor() {
                    self.kind_filter = Some(kind);
                    self.recompute_view();
                    self.maybe_fetch_current()
                } else {
                    self.start_install()
                }
            }
            KeyCode::Esc => {
                if self.kind_filter.is_some() || self.source_filter.is_some() {
                    self.kind_filter = None;
                    self.source_filter = None;
                    self.recompute_view();
                    self.maybe_fetch_current()
                } else {
                    AppCommand::None
                }
            }
            KeyCode::Char('s') => {
                if self.loading == LoadingState::Idle {
                    self.loading = LoadingState::Syncing;
                    self.mode = Mode::SyncProgress;
                    self.sync_result = None;
                    self.sync_result_scroll = 0;
                    AppCommand::Sync
                } else {
                    AppCommand::None
                }
            }
            KeyCode::Char('c') => {
                if let Some(def) = &self.selected_definition {
                    AppCommand::CopyBody(def.body.clone())
                } else {
                    AppCommand::None
                }
            }
            _ => AppCommand::None,
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.search_query.clear();
                self.recompute_view();
                AppCommand::None
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                // Keep the current filter active.
                self.maybe_fetch_current()
            }
            KeyCode::Down => {
                self.move_cursor_down();
                self.maybe_fetch_current()
            }
            KeyCode::Up => {
                self.move_cursor_up();
                self.maybe_fetch_current()
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.recompute_view();
                self.maybe_fetch_current()
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.recompute_view();
                self.maybe_fetch_current()
            }
            _ => AppCommand::None,
        }
    }

    fn move_cursor_down(&mut self) {
        self.cursor = grouping::next_item_index(&self.flat_items, self.cursor);
    }

    fn move_cursor_up(&mut self) {
        self.cursor = grouping::prev_item_index(&self.flat_items, self.cursor);
    }

    fn move_cursor_down_n(&mut self, n: usize) {
        for _ in 0..n {
            self.cursor = grouping::next_item_index(&self.flat_items, self.cursor);
        }
    }

    fn move_cursor_up_n(&mut self, n: usize) {
        for _ in 0..n {
            self.cursor = grouping::prev_item_index(&self.flat_items, self.cursor);
        }
    }

    fn scroll_detail_down(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_add(5);
    }

    fn scroll_detail_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(5);
    }

    fn scroll_detail_down_n(&mut self, n: u16) {
        self.detail_scroll = self.detail_scroll.saturating_add(n);
    }

    fn scroll_detail_up_n(&mut self, n: u16) {
        self.detail_scroll = self.detail_scroll.saturating_sub(n);
    }

    /// Check if a click at (col, row) is a double-click based on timing and position.
    /// Updates the last click state and returns true if this is a double-click.
    fn is_double_click(&mut self, col: u16, row: u16) -> bool {
        let now = Instant::now();
        let is_double = if let (Some(last_time), Some((last_col, last_row))) =
            (self.last_click_time, self.last_click_pos)
        {
            let elapsed = now.duration_since(last_time).as_millis();
            elapsed < DOUBLE_CLICK_THRESHOLD_MS && col == last_col && row == last_row
        } else {
            false
        };

        // Update last click state
        if is_double {
            // Reset after double-click to prevent triple-click being detected as double
            self.last_click_time = None;
            self.last_click_pos = None;
        } else {
            self.last_click_time = Some(now);
            self.last_click_pos = Some((col, row));
        }

        is_double
    }

    fn handle_normal_mouse(&mut self, mouse: MouseEvent) -> AppCommand {
        let pos = Position::new(mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let is_double = self.is_double_click(mouse.column, mouse.row);

                if self.layout_geometry.list_inner.contains(pos) {
                    if is_double {
                        // Double-click: open install dialogue if on an item
                        self.handle_list_click(mouse.row);
                        if self.selected_summary().is_some() {
                            return self.start_install();
                        }
                        AppCommand::None
                    } else {
                        self.handle_list_click(mouse.row)
                    }
                } else {
                    AppCommand::None
                }
            }
            MouseEventKind::ScrollDown => {
                if self.layout_geometry.list_inner.contains(pos) {
                    self.move_cursor_down_n(3);
                    self.maybe_fetch_current()
                } else if self.layout_geometry.detail_inner.contains(pos) {
                    self.scroll_detail_down_n(3);
                    AppCommand::None
                } else {
                    AppCommand::None
                }
            }
            MouseEventKind::ScrollUp => {
                if self.layout_geometry.list_inner.contains(pos) {
                    self.move_cursor_up_n(3);
                    self.maybe_fetch_current()
                } else if self.layout_geometry.detail_inner.contains(pos) {
                    self.scroll_detail_up_n(3);
                    AppCommand::None
                } else {
                    AppCommand::None
                }
            }
            _ => AppCommand::None,
        }
    }

    fn handle_list_click(&mut self, row: u16) -> AppCommand {
        let inner = self.layout_geometry.list_inner;
        let relative_row = (row.saturating_sub(inner.y)) as usize;
        let list_index = self.list_scroll_offset + relative_row;

        if list_index >= self.flat_items.len() {
            return AppCommand::None;
        }

        // Move cursor to clicked item.
        self.cursor = list_index;

        // If header, filter by kind; if item, just fetch the definition.
        if let Some(kind) = self.header_kind_at_cursor() {
            self.kind_filter = Some(kind);
            self.recompute_view();
            self.maybe_fetch_current()
        } else {
            self.maybe_fetch_current()
        }
    }

    fn handle_kind_filter_key(&mut self, key: KeyEvent) -> AppCommand {
        let kind_count = self.available_kinds().len();
        // Option count: "All" + each kind
        let option_count = 1 + kind_count;

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if option_count > 0 && self.kind_filter_cursor + 1 < option_count {
                    self.kind_filter_cursor += 1;
                }
                AppCommand::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.kind_filter_cursor = self.kind_filter_cursor.saturating_sub(1);
                AppCommand::None
            }
            KeyCode::Enter => {
                let kinds = self.available_kinds();
                if self.kind_filter_cursor == 0 {
                    // "All" selected
                    self.kind_filter = None;
                } else if let Some(kind) = kinds.get(self.kind_filter_cursor - 1) {
                    self.kind_filter = Some(kind.clone());
                }
                self.mode = Mode::Normal;
                self.recompute_view();
                self.maybe_fetch_current()
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_install_prompt_key(&mut self, key: KeyEvent) -> AppCommand {
        let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            // i: show confirmation dialog
            // I (shift+i): install immediately without confirmation
            KeyCode::Char('i') | KeyCode::Char('I') => {
                if let Some(explorer) = &self.file_explorer {
                    // Use cwd() to get the directory being browsed
                    let target = explorer.cwd().clone();
                    self.install_target = Some(target.clone());

                    // Compute the install path for preview/confirmation
                    if let Some(def) = &self.selected_definition {
                        let install_path = agent_defs::install::install_path(&target, def);
                        self.pending_install_path = Some(install_path);
                    }

                    if has_shift {
                        // Install immediately without confirmation
                        self.file_explorer = None;
                        self.pending_install_path = None;
                        self.mode = Mode::Normal;
                        return self.emit_install();
                    } else {
                        // Show confirmation dialog
                        self.mode = Mode::InstallConfirm;
                    }
                }
                AppCommand::None
            }
            KeyCode::Esc => {
                self.file_explorer = None;
                self.mode = Mode::Normal;
                AppCommand::None
            }
            _ => {
                // Pass other keys to the file explorer for navigation
                if let Some(explorer) = &mut self.file_explorer {
                    let event = Event::Key(key);
                    let _ = explorer.handle(&event);
                }
                AppCommand::None
            }
        }
    }

    fn handle_install_confirm_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                // Confirm installation
                self.file_explorer = None;
                self.pending_install_path = None;
                self.mode = Mode::Normal;
                self.emit_install()
            }
            KeyCode::Esc | KeyCode::Char('n') => {
                // Cancel - go back to explorer
                self.pending_install_path = None;
                self.install_target = None;
                self.mode = Mode::InstallPrompt;
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_source_filter_key(&mut self, key: KeyEvent) -> AppCommand {
        let sources = self.available_sources();
        // Option count: "All" + each source
        let option_count = 1 + sources.len();

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if option_count > 0 && self.source_filter_cursor + 1 < option_count {
                    self.source_filter_cursor += 1;
                }
                AppCommand::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.source_filter_cursor = self.source_filter_cursor.saturating_sub(1);
                AppCommand::None
            }
            KeyCode::Enter => {
                if self.source_filter_cursor == 0 {
                    // "All" selected
                    self.source_filter = None;
                } else if let Some(source) = sources.get(self.source_filter_cursor - 1) {
                    self.source_filter = Some(source.clone());
                }
                self.mode = Mode::Normal;
                self.recompute_view();
                self.maybe_fetch_current()
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_sync_progress_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                if let Some(result) = &self.sync_result {
                    self.set_status(result.message.clone(), false);
                }
                AppCommand::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.sync_result_scroll = self.sync_result_scroll.saturating_add(1);
                AppCommand::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.sync_result_scroll = self.sync_result_scroll.saturating_sub(1);
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_kind_filter_mouse(&mut self, mouse: MouseEvent) -> AppCommand {
        let pos = Position::new(mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(overlay) = self.layout_geometry.overlay {
                    if overlay.contains(pos) {
                        // Map click to option index (accounting for border).
                        let relative_row = mouse.row.saturating_sub(overlay.y + 1) as usize;
                        let kinds = self.available_kinds();
                        let option_count = 1 + kinds.len();

                        if relative_row < option_count {
                            self.kind_filter_cursor = relative_row;
                            // Apply selection (same as Enter key).
                            if self.kind_filter_cursor == 0 {
                                self.kind_filter = None;
                            } else if let Some(kind) = kinds.get(self.kind_filter_cursor - 1) {
                                self.kind_filter = Some(kind.clone());
                            }
                            self.mode = Mode::Normal;
                            self.recompute_view();
                            return self.maybe_fetch_current();
                        }
                    } else {
                        // Click outside: close overlay.
                        self.mode = Mode::Normal;
                    }
                }
                AppCommand::None
            }
            MouseEventKind::ScrollDown => {
                let option_count = 1 + self.available_kinds().len();
                if self.kind_filter_cursor + 1 < option_count {
                    self.kind_filter_cursor += 1;
                }
                AppCommand::None
            }
            MouseEventKind::ScrollUp => {
                self.kind_filter_cursor = self.kind_filter_cursor.saturating_sub(1);
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_source_filter_mouse(&mut self, mouse: MouseEvent) -> AppCommand {
        let pos = Position::new(mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(overlay) = self.layout_geometry.overlay {
                    if overlay.contains(pos) {
                        // Map click to option index (accounting for border).
                        let relative_row = mouse.row.saturating_sub(overlay.y + 1) as usize;
                        let sources = self.available_sources();
                        let option_count = 1 + sources.len();

                        if relative_row < option_count {
                            self.source_filter_cursor = relative_row;
                            // Apply selection (same as Enter key).
                            if self.source_filter_cursor == 0 {
                                self.source_filter = None;
                            } else if let Some(source) = sources.get(self.source_filter_cursor - 1)
                            {
                                self.source_filter = Some(source.clone());
                            }
                            self.mode = Mode::Normal;
                            self.recompute_view();
                            return self.maybe_fetch_current();
                        }
                    } else {
                        // Click outside: close overlay.
                        self.mode = Mode::Normal;
                    }
                }
                AppCommand::None
            }
            MouseEventKind::ScrollDown => {
                let option_count = 1 + self.available_sources().len();
                if self.source_filter_cursor + 1 < option_count {
                    self.source_filter_cursor += 1;
                }
                AppCommand::None
            }
            MouseEventKind::ScrollUp => {
                self.source_filter_cursor = self.source_filter_cursor.saturating_sub(1);
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_sync_progress_mouse(&mut self, mouse: MouseEvent) -> AppCommand {
        let pos = Position::new(mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(overlay) = self.layout_geometry.overlay
                    && !overlay.contains(pos)
                {
                    // Click outside: close overlay.
                    self.mode = Mode::Normal;
                    if let Some(result) = &self.sync_result {
                        self.set_status(result.message.clone(), false);
                    }
                }
                AppCommand::None
            }
            MouseEventKind::ScrollDown => {
                self.sync_result_scroll = self.sync_result_scroll.saturating_add(1);
                AppCommand::None
            }
            MouseEventKind::ScrollUp => {
                self.sync_result_scroll = self.sync_result_scroll.saturating_sub(1);
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_install_confirm_mouse(&mut self, mouse: MouseEvent) -> AppCommand {
        let pos = Position::new(mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(overlay) = self.layout_geometry.overlay
                    && !overlay.contains(pos)
                {
                    // Click outside: cancel and go back to explorer.
                    self.pending_install_path = None;
                    self.install_target = None;
                    self.mode = Mode::InstallPrompt;
                }
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_install_prompt_mouse(&mut self, mouse: MouseEvent) -> AppCommand {
        let pos = Position::new(mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let is_double = self.is_double_click(mouse.column, mouse.row);

                if let Some(overlay) = self.layout_geometry.overlay {
                    if !overlay.contains(pos) {
                        // Click outside: close explorer.
                        self.file_explorer = None;
                        self.mode = Mode::Normal;
                    } else if let Some(list_inner) = self.layout_geometry.explorer_list_inner
                        && list_inner.contains(pos)
                    {
                        // Click inside file list: select the clicked item.
                        self.handle_explorer_click(mouse.row, list_inner);

                        // Double-click: navigate into folder if it's a directory.
                        if is_double
                            && let Some(explorer) = &mut self.file_explorer
                            && explorer.current().is_dir()
                        {
                            let _ = explorer.handle(Input::Right);
                        }
                    }
                }
                AppCommand::None
            }
            MouseEventKind::ScrollDown => {
                // Pass scroll to file explorer as PageDown for faster navigation.
                if let Some(explorer) = &mut self.file_explorer {
                    let _ = explorer.handle(Input::PageDown);
                }
                AppCommand::None
            }
            MouseEventKind::ScrollUp => {
                // Pass scroll to file explorer as PageUp for faster navigation.
                if let Some(explorer) = &mut self.file_explorer {
                    let _ = explorer.handle(Input::PageUp);
                }
                AppCommand::None
            }
            _ => AppCommand::None,
        }
    }

    fn handle_explorer_click(&mut self, row: u16, list_inner: Rect) {
        let Some(explorer) = &mut self.file_explorer else {
            return;
        };

        let files_count = explorer.files().len();
        if files_count == 0 {
            return;
        }

        let visible_height = list_inner.height as usize;
        let selected_idx = explorer.selected_idx();

        // Estimate scroll offset using minimal scrolling logic (matches ratatui's List behavior).
        // The List widget only scrolls when the selection goes out of view:
        // - If selection < scroll, scroll = selection (scroll up to show selection at top)
        // - If selection >= scroll + height, scroll = selection - height + 1 (scroll down)
        // - Otherwise, scroll stays where it was
        //
        // Since we don't track the scroll state, we estimate based on selection position:
        let scroll_offset = if files_count <= visible_height {
            // All items fit, no scrolling needed
            0
        } else if selected_idx < visible_height {
            // Selection is in the first "page", assume no scrolling has happened yet
            0
        } else if selected_idx >= files_count.saturating_sub(visible_height) {
            // Selection is in the last "page", scroll to show the end
            files_count.saturating_sub(visible_height)
        } else {
            // Selection is in the middle - assume it's at the bottom of the viewport
            // (this is what happens with minimal scrolling when navigating down)
            selected_idx.saturating_sub(visible_height - 1)
        };

        // Calculate which file index was clicked
        let relative_row = (row.saturating_sub(list_inner.y)) as usize;
        let target_idx = scroll_offset + relative_row;

        // Set selection if valid
        if target_idx < files_count {
            explorer.set_selected_idx(target_idx);
        }
    }

    fn start_install(&mut self) -> AppCommand {
        if self.selected_definition.is_none() {
            return AppCommand::None;
        }

        // Always show the file explorer for directory selection
        let theme = Theme::default().add_default_title();
        match FileExplorer::with_theme(theme) {
            Ok(mut explorer) => {
                // If we have a previous target, start there
                if let Some(target) = &self.install_target {
                    let _ = explorer.set_cwd(target);
                }
                self.file_explorer = Some(explorer);
                self.mode = Mode::InstallPrompt;
            }
            Err(e) => {
                self.set_status(format!("Failed to open file explorer: {e}"), true);
            }
        }
        AppCommand::None
    }

    fn emit_install(&mut self) -> AppCommand {
        let Some(def) = &self.selected_definition else {
            return AppCommand::None;
        };
        let Some(target) = &self.install_target else {
            return AppCommand::None;
        };
        let install_path = agent_defs::install::install_path(target, def);
        AppCommand::Install {
            raw: def.raw.clone(),
            install_path,
        }
    }

    /// Get the DefinitionKind if the cursor is on a header row.
    fn header_kind_at_cursor(&self) -> Option<DefinitionKind> {
        let row = self.flat_items.get(self.cursor)?;
        if let ListRow::Header { label, .. } = row {
            // Find the group matching this label.
            self.groups
                .iter()
                .find(|g| &g.label == label)
                .map(|g| g.kind.clone())
        } else {
            None
        }
    }

    /// Get the distinct kinds present in the full (unfiltered) summaries.
    pub fn available_kinds(&self) -> Vec<DefinitionKind> {
        let mut kinds: Vec<DefinitionKind> = Vec::new();
        for s in &self.summaries {
            if !kinds.contains(&s.kind) {
                kinds.push(s.kind.clone());
            }
        }
        kinds.sort_by_key(grouping::kind_sort_key);
        kinds
    }

    /// Get the distinct source labels present in the full (unfiltered) summaries.
    pub fn available_sources(&self) -> Vec<String> {
        let mut sources: Vec<String> = Vec::new();
        for s in &self.summaries {
            if !sources.contains(&s.source_label) {
                sources.push(s.source_label.clone());
            }
        }
        sources.sort();
        sources
    }

    /// If the cursor is on a selectable item, return a fetch command.
    fn maybe_fetch_current(&mut self) -> AppCommand {
        if let Some(summary) = self.selected_summary() {
            let id = summary.id.clone();
            // Don't re-fetch if we already have this one or it's already pending.
            if self.pending_fetch.as_ref() == Some(&id) {
                return AppCommand::None;
            }
            if let Some(def) = &self.selected_definition
                && def.id == id
            {
                return AppCommand::None;
            }
            self.pending_fetch = Some(id.clone());
            self.loading = LoadingState::Fetching;
            AppCommand::FetchDefinition(id)
        } else {
            AppCommand::None
        }
    }

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

    fn set_status(&mut self, text: String, is_error: bool) {
        self.status_message = Some(StatusMessage {
            text,
            is_error,
            ticks_remaining: 12, // ~3 seconds at 250ms tick
        });
    }
}

#[cfg(test)]
mod tests {
    use agent_defs::{DefinitionId, DefinitionKind};
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::*;
    use crate::action::{Action, AppCommand};

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

    fn summary_with_desc(name: &str, desc: &str, kind: DefinitionKind) -> DefinitionSummary {
        DefinitionSummary {
            id: DefinitionId::new(name),
            name: name.to_owned(),
            description: Some(desc.to_owned()),
            kind,
            category: None,
            source_label: "test".into(),
        }
    }

    fn key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn ctrl_key_event(c: char) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn sample_definition(name: &str) -> Definition {
        Definition {
            id: DefinitionId::new(name),
            name: name.to_owned(),
            description: None,
            kind: DefinitionKind::Agent,
            category: None,
            source_label: "test".to_owned(),
            body: format!("Body of {name}"),
            tools: vec![],
            model: None,
            metadata: std::collections::HashMap::new(),
            raw: String::new(),
        }
    }

    // --- Construction ---

    #[test]
    fn new_with_empty_summaries() {
        let app = App::new(vec![], "test".into());
        assert!(app.groups.is_empty());
        assert!(app.flat_items.is_empty());
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn new_with_mixed_summaries_groups_correctly() {
        let summaries = vec![
            summary("skill1", DefinitionKind::Skill),
            summary("agent1", DefinitionKind::Agent),
            summary("agent2", DefinitionKind::Agent),
        ];

        let app = App::new(summaries, "test".into());
        assert_eq!(app.groups.len(), 2);
        assert_eq!(app.groups[0].label, "Agents");
        assert_eq!(app.groups[1].label, "Skills");
    }

    #[test]
    fn new_places_cursor_on_first_item() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Hook),
        ];

        let app = App::new(summaries, "test".into());
        // Cursor should be on first item (index 1, after Header at 0).
        assert_eq!(app.cursor, 1);
        assert!(matches!(app.flat_items[app.cursor], ListRow::Item { .. }));
    }

    // --- Navigation ---

    #[test]
    fn cursor_down_moves_to_next_item() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Agent),
        ];

        let mut app = App::new(summaries, "test".into());
        // Pre-seed selected so fetch doesn't fire for 'a'
        app.selected_definition = Some(sample_definition("a"));
        app.pending_fetch = None;

        let initial = app.cursor;
        app.handle_event(key_event(KeyCode::Char('j')));
        assert!(app.cursor > initial);
    }

    #[test]
    fn cursor_up_at_top_stays() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Agent),
        ];

        let mut app = App::new(summaries, "test".into());
        let initial = app.cursor; // first item
        app.handle_event(key_event(KeyCode::Char('k')));
        assert_eq!(app.cursor, initial);
    }

    #[test]
    fn cursor_down_at_bottom_stays() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];

        let mut app = App::new(summaries, "test".into());
        app.selected_definition = Some(sample_definition("a"));
        let initial = app.cursor;
        app.handle_event(key_event(KeyCode::Char('j')));
        assert_eq!(app.cursor, initial);
    }

    #[test]
    fn cursor_skips_headers_between_groups() {
        let summaries = vec![
            summary("agent1", DefinitionKind::Agent),
            summary("hook1", DefinitionKind::Hook),
        ];

        let mut app = App::new(summaries, "test".into());
        app.selected_definition = Some(sample_definition("agent1"));
        app.pending_fetch = None;

        // Cursor starts at Agent item (idx 1).
        // flat: Header(Agents), Item(agent1), Header(Hooks), Item(hook1)
        assert_eq!(app.cursor, 1);
        app.handle_event(key_event(KeyCode::Down));
        // Should jump to idx 3 (hook1), skipping Header(Hooks) at idx 2.
        assert_eq!(app.cursor, 3);
    }

    // --- Quit ---

    #[test]
    fn q_returns_quit() {
        let mut app = App::new(vec![], "test".into());
        let cmd = app.handle_event(key_event(KeyCode::Char('q')));
        assert!(matches!(cmd, AppCommand::Quit));
    }

    #[test]
    fn ctrl_c_returns_quit() {
        let mut app = App::new(vec![], "test".into());
        let cmd = app.handle_event(ctrl_key_event('c'));
        assert!(matches!(cmd, AppCommand::Quit));
    }

    // --- Fetch ---

    #[test]
    fn cursor_change_triggers_fetch() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Agent),
        ];

        let mut app = App::new(summaries, "test".into());
        // Clear initial fetch state.
        app.pending_fetch = None;
        app.selected_definition = Some(sample_definition("a"));
        app.loading = LoadingState::Idle;

        let cmd = app.handle_event(key_event(KeyCode::Char('j')));
        assert!(matches!(cmd, AppCommand::FetchDefinition(_)));
    }

    #[test]
    fn definition_loaded_updates_selected() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let mut app = App::new(summaries, "test".into());
        app.pending_fetch = Some(DefinitionId::new("a"));

        let def = sample_definition("a");
        let cmd = app.handle_action(Action::DefinitionLoaded(
            DefinitionId::new("a"),
            Box::new(Ok(def.clone())),
        ));

        assert!(matches!(cmd, AppCommand::None));
        assert!(app.selected_definition.is_some());
        assert_eq!(app.selected_definition.unwrap().name, "a");
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn stale_fetch_is_silently_dropped() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Agent),
        ];

        let mut app = App::new(summaries, "test".into());
        // Pending fetch is for "b", but we get a response for "a".
        app.pending_fetch = Some(DefinitionId::new("b"));

        let def = sample_definition("a");
        app.handle_action(Action::DefinitionLoaded(DefinitionId::new("a"), Box::new(Ok(def))));

        // selected_definition should not have been set.
        assert!(app.selected_definition.is_none());
    }

    // --- Detail scroll ---

    #[test]
    fn page_down_increases_detail_scroll() {
        let mut app = App::new(vec![], "test".into());
        app.handle_event(key_event(KeyCode::PageDown));
        assert!(app.detail_scroll > 0);
    }

    #[test]
    fn page_up_at_zero_stays() {
        let mut app = App::new(vec![], "test".into());
        app.handle_event(key_event(KeyCode::PageUp));
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn ctrl_d_scrolls_detail_down() {
        let mut app = App::new(vec![], "test".into());
        app.handle_event(ctrl_key_event('d'));
        assert!(app.detail_scroll > 0);
    }

    #[test]
    fn ctrl_u_scrolls_detail_up() {
        let mut app = App::new(vec![], "test".into());
        app.detail_scroll = 10;
        app.handle_event(ctrl_key_event('u'));
        assert!(app.detail_scroll < 10);
    }

    // --- Search mode ---

    #[test]
    fn slash_enters_search_mode() {
        let mut app = App::new(vec![], "test".into());
        app.handle_event(key_event(KeyCode::Char('/')));
        assert_eq!(app.mode, Mode::Search);
    }

    #[test]
    fn typing_in_search_appends_to_query() {
        let summaries = vec![
            summary_with_desc("Code Architect", "designs architecture", DefinitionKind::Agent),
            summary("Test Runner", DefinitionKind::Agent),
        ];

        let mut app = App::new(summaries, "test".into());
        app.handle_event(key_event(KeyCode::Char('/')));
        app.handle_event(key_event(KeyCode::Char('a')));
        app.handle_event(key_event(KeyCode::Char('r')));
        app.handle_event(key_event(KeyCode::Char('c')));
        app.handle_event(key_event(KeyCode::Char('h')));

        assert_eq!(app.search_query, "arch");
        // Should filter to only Code Architect (matches description).
        let item_count = app
            .flat_items
            .iter()
            .filter(|r| matches!(r, ListRow::Item { .. }))
            .count();
        assert_eq!(item_count, 1);
    }

    #[test]
    fn escape_clears_search_and_restores_list() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Agent),
        ];

        let mut app = App::new(summaries, "test".into());
        let original_count = app.flat_items.len();

        app.handle_event(key_event(KeyCode::Char('/')));
        app.handle_event(key_event(KeyCode::Char('z'))); // No match
        assert_ne!(app.flat_items.len(), original_count);

        app.handle_event(key_event(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.search_query.is_empty());
        assert_eq!(app.flat_items.len(), original_count);
    }

    #[test]
    fn enter_confirms_search_returns_to_normal() {
        let mut app = App::new(vec![summary("a", DefinitionKind::Agent)], "test".into());
        app.handle_event(key_event(KeyCode::Char('/')));
        app.handle_event(key_event(KeyCode::Char('a')));
        app.handle_event(key_event(KeyCode::Enter));

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.search_query, "a"); // Preserved
    }

    #[test]
    fn backspace_in_search_removes_last_char() {
        let mut app = App::new(vec![], "test".into());
        app.handle_event(key_event(KeyCode::Char('/')));
        app.handle_event(key_event(KeyCode::Char('a')));
        app.handle_event(key_event(KeyCode::Char('b')));
        app.handle_event(key_event(KeyCode::Backspace));

        assert_eq!(app.search_query, "a");
    }

    // --- Sync ---

    #[test]
    fn s_triggers_sync() {
        let mut app = App::new(vec![], "test".into());
        let cmd = app.handle_event(key_event(KeyCode::Char('s')));
        assert!(matches!(cmd, AppCommand::Sync));
        assert_eq!(app.loading, LoadingState::Syncing);
    }

    #[test]
    fn s_during_loading_is_noop() {
        let mut app = App::new(vec![], "test".into());
        app.loading = LoadingState::Syncing;
        let cmd = app.handle_event(key_event(KeyCode::Char('s')));
        assert!(matches!(cmd, AppCommand::None));
    }

    #[test]
    fn sync_completed_triggers_reload() {
        let mut app = App::new(vec![], "test".into());
        app.loading = LoadingState::Syncing;
        app.mode = Mode::SyncProgress;

        let result = SyncResult {
            message: "Synced 5".into(),
            warnings: vec![],
        };
        let cmd = app.handle_action(Action::SyncCompleted(Ok(result)));
        assert!(matches!(cmd, AppCommand::ReloadList));
        assert_eq!(app.loading, LoadingState::Idle);
        // Should stay in SyncProgress mode to show results
        assert_eq!(app.mode, Mode::SyncProgress);
    }

    // --- Copy ---

    #[test]
    fn c_with_selection_returns_copy() {
        let mut app = App::new(vec![summary("a", DefinitionKind::Agent)], "test".into());
        app.selected_definition = Some(sample_definition("a"));

        let cmd = app.handle_event(key_event(KeyCode::Char('c')));
        assert!(matches!(cmd, AppCommand::CopyBody(_)));
    }

    #[test]
    fn c_without_selection_is_noop() {
        let mut app = App::new(vec![], "test".into());
        let cmd = app.handle_event(key_event(KeyCode::Char('c')));
        assert!(matches!(cmd, AppCommand::None));
    }

    // --- Reload ---

    #[test]
    fn reload_recomputes_groups() {
        let mut app = App::new(vec![summary("a", DefinitionKind::Agent)], "test".into());
        assert_eq!(app.groups.len(), 1);

        app.reload(vec![
            summary("x", DefinitionKind::Agent),
            summary("y", DefinitionKind::Hook),
        ]);

        assert_eq!(app.groups.len(), 2);
    }

    #[test]
    fn reload_preserves_search_filter() {
        let summaries = vec![
            summary("alpha", DefinitionKind::Agent),
            summary("beta", DefinitionKind::Agent),
        ];

        let mut app = App::new(summaries, "test".into());
        // Enter search and filter to "alpha"
        app.handle_event(key_event(KeyCode::Char('/')));
        app.handle_event(key_event(KeyCode::Char('a')));
        app.handle_event(key_event(KeyCode::Char('l')));
        app.handle_event(key_event(KeyCode::Enter));

        // Reload with same data.
        app.reload(vec![
            summary("alpha", DefinitionKind::Agent),
            summary("beta", DefinitionKind::Agent),
            summary("gamma", DefinitionKind::Agent),
        ]);

        // Filter "al" should still be active.
        let item_count = app
            .flat_items
            .iter()
            .filter(|r| matches!(r, ListRow::Item { .. }))
            .count();
        assert_eq!(item_count, 1); // Only "alpha" matches "al"
    }

    // --- Tick ---

    #[test]
    fn tick_expires_status_message() {
        let mut app = App::new(vec![], "test".into());
        app.status_message = Some(StatusMessage {
            text: "hello".into(),
            is_error: false,
            ticks_remaining: 1,
        });

        app.tick(); // ticks_remaining -> 0
        assert!(app.status_message.is_some());

        app.tick(); // expires
        assert!(app.status_message.is_none());
    }

    // --- Kind filter ---

    #[test]
    fn f_enters_kind_filter_mode() {
        let mut app = App::new(vec![summary("a", DefinitionKind::Agent)], "test".into());
        app.handle_event(key_event(KeyCode::Char('f')));
        assert_eq!(app.mode, Mode::KindFilter);
        assert_eq!(app.kind_filter_cursor, 0);
    }

    #[test]
    fn kind_filter_jk_moves_cursor() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Hook),
        ];
        let mut app = App::new(summaries, "test".into());
        app.handle_event(key_event(KeyCode::Char('f')));

        // Options: All(0), Agents(1), Hooks(2)
        assert_eq!(app.kind_filter_cursor, 0);

        app.handle_event(key_event(KeyCode::Char('j')));
        assert_eq!(app.kind_filter_cursor, 1);

        app.handle_event(key_event(KeyCode::Char('j')));
        assert_eq!(app.kind_filter_cursor, 2);

        // At bottom, stays
        app.handle_event(key_event(KeyCode::Char('j')));
        assert_eq!(app.kind_filter_cursor, 2);

        app.handle_event(key_event(KeyCode::Char('k')));
        assert_eq!(app.kind_filter_cursor, 1);

        app.handle_event(key_event(KeyCode::Char('k')));
        assert_eq!(app.kind_filter_cursor, 0);

        // At top, stays
        app.handle_event(key_event(KeyCode::Char('k')));
        assert_eq!(app.kind_filter_cursor, 0);
    }

    #[test]
    fn kind_filter_enter_applies_filter() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Hook),
            summary("c", DefinitionKind::Agent),
        ];
        let mut app = App::new(summaries, "test".into());

        // Enter kind filter, select Agents (cursor 1)
        app.handle_event(key_event(KeyCode::Char('f')));
        app.handle_event(key_event(KeyCode::Char('j'))); // Agents
        app.handle_event(key_event(KeyCode::Enter));

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.kind_filter, Some(DefinitionKind::Agent));

        // Should only show agents
        let item_count = app
            .flat_items
            .iter()
            .filter(|r| matches!(r, ListRow::Item { .. }))
            .count();
        assert_eq!(item_count, 2); // a and c
    }

    #[test]
    fn kind_filter_all_clears_filter() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Hook),
        ];
        let mut app = App::new(summaries, "test".into());

        // Set a filter first
        app.kind_filter = Some(DefinitionKind::Agent);
        app.recompute_view();
        let filtered_count = app
            .flat_items
            .iter()
            .filter(|r| matches!(r, ListRow::Item { .. }))
            .count();
        assert_eq!(filtered_count, 1);

        // Open filter, select All (cursor 0)
        app.handle_event(key_event(KeyCode::Char('f')));
        app.handle_event(key_event(KeyCode::Enter)); // cursor is at 0 = All

        assert_eq!(app.kind_filter, None);
        let item_count = app
            .flat_items
            .iter()
            .filter(|r| matches!(r, ListRow::Item { .. }))
            .count();
        assert_eq!(item_count, 2);
    }

    #[test]
    fn kind_filter_esc_cancels() {
        let mut app = App::new(vec![summary("a", DefinitionKind::Agent)], "test".into());
        app.handle_event(key_event(KeyCode::Char('f')));
        assert_eq!(app.mode, Mode::KindFilter);

        app.handle_event(key_event(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn enter_on_header_sets_kind_filter() {
        let summaries = vec![
            summary("agent1", DefinitionKind::Agent),
            summary("hook1", DefinitionKind::Hook),
        ];
        let mut app = App::new(summaries, "test".into());

        // flat: Header(Agents)=0, Item(agent1)=1, Header(Hooks)=2, Item(hook1)=3
        // Move cursor to the Header(Agents) row (index 0)
        // We need to manually set cursor to a header since navigation skips them
        app.cursor = 0;
        app.handle_event(key_event(KeyCode::Enter));

        assert_eq!(app.kind_filter, Some(DefinitionKind::Agent));
        let item_count = app
            .flat_items
            .iter()
            .filter(|r| matches!(r, ListRow::Item { .. }))
            .count();
        assert_eq!(item_count, 1); // Only agent1
    }

    #[test]
    fn esc_clears_active_kind_filter() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Hook),
        ];
        let mut app = App::new(summaries, "test".into());

        // Set filter
        app.kind_filter = Some(DefinitionKind::Agent);
        app.recompute_view();
        assert_eq!(
            app.flat_items
                .iter()
                .filter(|r| matches!(r, ListRow::Item { .. }))
                .count(),
            1
        );

        // Esc clears it
        app.handle_event(key_event(KeyCode::Esc));
        assert_eq!(app.kind_filter, None);
        assert_eq!(
            app.flat_items
                .iter()
                .filter(|r| matches!(r, ListRow::Item { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn esc_without_filter_is_noop() {
        let mut app = App::new(vec![summary("a", DefinitionKind::Agent)], "test".into());
        let cmd = app.handle_event(key_event(KeyCode::Esc));
        assert!(matches!(cmd, AppCommand::None));
        assert_eq!(app.kind_filter, None);
    }

    #[test]
    fn kind_filter_plus_search_combines() {
        let summaries = vec![
            summary_with_desc("alpha-agent", "desc", DefinitionKind::Agent),
            summary_with_desc("beta-agent", "desc", DefinitionKind::Agent),
            summary("gamma-hook", DefinitionKind::Hook),
        ];
        let mut app = App::new(summaries, "test".into());

        // Set kind filter to Agent
        app.kind_filter = Some(DefinitionKind::Agent);
        // Set search query
        app.search_query = "alpha".into();
        app.recompute_view();

        let item_count = app
            .flat_items
            .iter()
            .filter(|r| matches!(r, ListRow::Item { .. }))
            .count();
        assert_eq!(item_count, 1); // Only alpha-agent matches both filters
    }

    #[test]
    fn reload_preserves_kind_filter() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Hook),
        ];
        let mut app = App::new(summaries, "test".into());

        // Set kind filter
        app.kind_filter = Some(DefinitionKind::Agent);
        app.recompute_view();

        // Reload with new data
        app.reload(vec![
            summary("x", DefinitionKind::Agent),
            summary("y", DefinitionKind::Hook),
            summary("z", DefinitionKind::Agent),
        ]);

        // Filter should still be active
        assert_eq!(app.kind_filter, Some(DefinitionKind::Agent));
        let item_count = app
            .flat_items
            .iter()
            .filter(|r| matches!(r, ListRow::Item { .. }))
            .count();
        assert_eq!(item_count, 2); // x and z
    }

    #[test]
    fn recompute_view_applies_kind_filter() {
        let summaries = vec![
            summary("a", DefinitionKind::Agent),
            summary("b", DefinitionKind::Hook),
            summary("c", DefinitionKind::Skill),
        ];
        let mut app = App::new(summaries, "test".into());

        app.kind_filter = Some(DefinitionKind::Hook);
        app.recompute_view();

        assert_eq!(app.groups.len(), 1);
        assert_eq!(app.groups[0].label, "Hooks");
        assert_eq!(
            app.flat_items
                .iter()
                .filter(|r| matches!(r, ListRow::Item { .. }))
                .count(),
            1
        );
    }

    // --- Install ---

    fn sample_definition_with_raw(name: &str, raw: &str) -> Definition {
        Definition {
            id: DefinitionId::new(name),
            name: name.to_owned(),
            description: None,
            kind: DefinitionKind::Agent,
            category: None,
            source_label: "test".to_owned(),
            body: format!("Body of {name}"),
            tools: vec![],
            model: None,
            metadata: std::collections::HashMap::new(),
            raw: raw.to_owned(),
        }
    }

    fn shift_key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn i_with_target_opens_explorer_at_target() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let mut app =
            App::with_install_target(summaries, "test".into(), Some(PathBuf::from("/tmp")));
        app.selected_definition = Some(sample_definition_with_raw("a", "raw content"));

        // Pressing 'i' now always opens the explorer (even with existing target)
        let cmd = app.handle_event(key_event(KeyCode::Char('i')));
        assert!(matches!(cmd, AppCommand::None));
        assert_eq!(app.mode, Mode::InstallPrompt);
        assert!(app.file_explorer.is_some());
    }

    #[test]
    fn i_without_target_enters_install_prompt() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let mut app = App::new(summaries, "test".into());
        app.selected_definition = Some(sample_definition_with_raw("a", "raw content"));

        let cmd = app.handle_event(key_event(KeyCode::Char('i')));
        assert!(matches!(cmd, AppCommand::None));
        assert_eq!(app.mode, Mode::InstallPrompt);
        assert!(app.file_explorer.is_some());
    }

    #[test]
    fn i_without_selection_is_noop() {
        let mut app = App::new(vec![], "test".into());
        let cmd = app.handle_event(key_event(KeyCode::Char('i')));
        assert!(matches!(cmd, AppCommand::None));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn install_prompt_esc_cancels() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let mut app = App::new(summaries, "test".into());
        app.selected_definition = Some(sample_definition_with_raw("a", "content"));

        app.handle_event(key_event(KeyCode::Char('i')));
        assert_eq!(app.mode, Mode::InstallPrompt);
        assert!(app.file_explorer.is_some());

        app.handle_event(key_event(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.file_explorer.is_none());
    }

    #[test]
    fn install_prompt_i_shows_confirmation() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let mut app = App::new(summaries, "test".into());
        app.selected_definition = Some(sample_definition_with_raw("a", "content"));

        app.handle_event(key_event(KeyCode::Char('i')));
        assert_eq!(app.mode, Mode::InstallPrompt);

        // Press 'i' again to show confirmation dialog
        app.handle_event(key_event(KeyCode::Char('i')));
        assert_eq!(app.mode, Mode::InstallConfirm);
        assert!(app.install_target.is_some());
        assert!(app.pending_install_path.is_some());
    }

    #[test]
    fn install_prompt_shift_i_installs_immediately() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let mut app = App::new(summaries, "test".into());
        app.selected_definition = Some(sample_definition_with_raw("a", "content"));

        app.handle_event(key_event(KeyCode::Char('i')));
        assert_eq!(app.mode, Mode::InstallPrompt);

        // Press Shift+I to install immediately
        let cmd = app.handle_event(shift_key_event(KeyCode::Char('I')));
        assert!(matches!(cmd, AppCommand::Install { .. }));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.install_target.is_some());
        assert!(app.file_explorer.is_none());
    }

    #[test]
    fn install_confirm_enter_confirms() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let mut app = App::new(summaries, "test".into());
        app.selected_definition = Some(sample_definition_with_raw("a", "content"));
        app.install_target = Some(PathBuf::from("/tmp"));
        app.mode = Mode::InstallConfirm;

        let cmd = app.handle_event(key_event(KeyCode::Enter));
        assert!(matches!(cmd, AppCommand::Install { .. }));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn install_confirm_esc_returns_to_explorer() {
        let summaries = vec![summary("a", DefinitionKind::Agent)];
        let mut app = App::new(summaries, "test".into());
        app.selected_definition = Some(sample_definition_with_raw("a", "content"));
        app.install_target = Some(PathBuf::from("/tmp"));
        app.mode = Mode::InstallConfirm;

        app.handle_event(key_event(KeyCode::Esc));
        assert_eq!(app.mode, Mode::InstallPrompt);
        assert!(app.install_target.is_none()); // Should be cleared
    }

    #[test]
    fn install_completed_ok_shows_status() {
        let mut app = App::new(vec![], "test".into());
        app.handle_action(Action::InstallCompleted(Ok("Installed to /tmp/test".into())));
        assert!(app.status_message.is_some());
        assert!(!app.status_message.as_ref().unwrap().is_error);
    }

    #[test]
    fn install_completed_err_shows_error() {
        let mut app = App::new(vec![], "test".into());
        app.handle_action(Action::InstallCompleted(Err("write failed".into())));
        assert!(app.status_message.is_some());
        assert!(app.status_message.as_ref().unwrap().is_error);
    }
}
