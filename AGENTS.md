# AGENTS.md

## Project

QuillCode — agentic coding assistant CLI built in Rust. Uses LLM inference (local GGUF models via llama-cpp-2 or OpenAI API) to execute coding tasks with tool automation.

## Validation

After every code change, run `cargo check --all-targets` (or `cargo test` when tests exist for the affected area). Never leave the codebase in a state that doesn't compile.

## Architecture

Layered: `domain/` → `infrastructure/` → `repository/`.

- **Domain** — business logic, tools (`Tool` trait), workflow orchestration, permissions, sessions, prompting
- **Infrastructure** — TUI (ratatui), event bus (crossbeam channels), inference engines, OpenAI client, OAuth, DB setup
- **Repository** — SQLite via rusqlite/r2d2, struct-per-table pattern

CLI thread and agent thread communicate via `EventBus` (`src/infrastructure/event_bus.rs`) — two unbounded crossbeam channels: `UiToAgentEvent` (user input, permissions, settings) and `AgentToUiEvent` (progress, permission requests, results).

## Key Files

| File | What it does |
|------|-------------|
| `src/main.rs` | Entry point — logging, infrastructure init, spawns agent & CLI threads |
| `src/lib.rs` | Public API — exports domain, infrastructure, repository modules |
| `src/infrastructure/event_controller.rs` | Agent-side event loop, bridges UI events to workflow |
| `src/infrastructure/cli/repl.rs` | CLI-side event loop, TUI init and input handling |
| `src/domain/workflow/workflow.rs` | Workflow engine — orchestrates tool execution per mode (Build/Plan/BuildFromPlan) |
| `src/domain/tools/` | Tool implementations: `find_files`, `read_objects`, `patch_files`, `shell_exec`, `web_search`, etc. |
| `src/domain/permissions/checker.rs` | Permission validation, dangerous command detection |
| `src/domain/session/service.rs` | Session lifecycle — routes Build/BuildFromPlan modes, orchestrates TODO-item sub-agents with user confirmation between items |
| `src/infrastructure/inference/openai.rs` | OpenAI inference engine |
| `src/infrastructure/inference/local.rs` | Local GGUF model inference (llama-cpp-2) |
| `src/infrastructure/db/` | SQLite connection pooling, schema migrations |
| `src/utils/parsing/` | Tree-sitter based code parsing (Rust, Python, JS, TS, Go, Java, C, C++, Ruby, etc.) |

## User Settings

`UserSettings` (`src/domain/user_settings.rs`) is a singleton (row id=1 in SQLite) that drives runtime behavior: which model to use (`current_model_id`), auth method (API key vs OAuth), whether web search and OpenAI tracing are enabled, `max_tool_calls_per_request` limit, and Brave API key. Updated live via `SettingsUpdateEvent` through the event bus.

## Conventions

- Follow existing naming: `*Row` for DB rows, `*Service` for domain services, `*Repository` for data access, `*Engine` for backends.
- Errors: `thiserror` in domain, `Box<dyn Error + Send + Sync>` in infrastructure, `Result<T, String>` in repositories.
- If some of the key files locations change, or new key functionality is added, feel free to update the Key Files table above.
