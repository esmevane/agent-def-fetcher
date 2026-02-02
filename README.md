# agent-def-fetcher

A CLI tool for fetching and browsing agent definitions from curated GitHub sources. Includes an interactive TUI for exploring definitions and installing them to your projects.

## Installation

```sh
cargo install --path crates/agent-defs-cli
```

## Usage

### Sync definitions from sources

```sh
agent-def-fetcher sync
```

Fetches definitions from all configured sources and caches them locally. The cache lives at `~/.cache/agent-def-fetcher/`.

### List definitions

```sh
agent-def-fetcher list
agent-def-fetcher list --kind agent
agent-def-fetcher list --source claude-code-templates
```

### Search definitions

```sh
agent-def-fetcher search "code review"
agent-def-fetcher search "test" --kind skill
```

### Show a definition

```sh
agent-def-fetcher show agents/code-reviewer.md
agent-def-fetcher show agents/code-reviewer.md --raw
```

### Install a definition

```sh
agent-def-fetcher install agents/code-reviewer.md --target ./my-project
```

### Interactive TUI

```sh
agent-def-fetcher tui
agent-def-fetcher tui --target ./my-project
```

The TUI provides:
- Browse definitions grouped by kind
- Filter by kind (press `k`) or source (press `s`)
- Search (press `/`)
- View full definition content with scrolling
- Install definitions to a directory (press `i`)
- Copy definition body to clipboard (press `y`)
- Sync from sources (press `S`)

Mouse support:
- Click to select items
- Scroll wheel to navigate lists
- Click outside overlays to close them
- Double-click to open/navigate in dialogs

## Sources

Sources are typically Github repos that have a bunch of agent-model-friendly configuration presets in them. Two sources exist right now:

- [davila7/claude-code-templates](https://github.com/davila7/claude-code-templates)
- [VoltAgent/awesome-claude-code-subagents](https://github.com/VoltAgent/awesome-claude-code-subagents)

## Environment Variables

- `GITHUB_TOKEN` - Optional. Increases API rate limits and enables access to private repositories.

## Definition Kinds

Definitions are categorized by kind:
- `agent` - AI agent definitions
- `command` - CLI commands
- `hook` - Event hooks
- `mcp` - MCP server configurations
- `setting` - Configuration settings
- `skill` - Reusable skills

## Building from source

```sh
cargo build --release
```

## Running tests

```sh
cargo test --workspace
```
