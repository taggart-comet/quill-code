# QuillCode

A coding agent I built as a side project. It's not trying to compete with Claude Code or Cursor — those are good. The goal was to understand what's actually inside one of these things by building it myself.

The result is a working terminal-based coding agent written in Rust. It runs a tool-calling loop against an LLM (OpenAI API or a local GGUF model), executes tools with a permission layer, maintains conversation history with context compression, and has a plan mode where it works through a TODO list item-by-item with your approval between steps.

---

## What it does

- **Agent loop** — calls the LLM, picks a tool, runs it, feeds the result back into the conversation, repeats
- **Terminal UI** — built with ratatui, two OS threads (one for UI, one for the agent), communicating over crossbeam channels
- **Eight tools**:
  - `find_files` — glob-based file search
  - `read_objects` — AST-aware code reading (line ranges or symbol names, not full-file dumps)
  - `patch_files` — edits via unified diffs
  - `shell_exec` — shell command execution
  - `web_search` — Brave Search API
  - `structure` — directory tree view
  - `discover_objects` — lists symbols in a file via Tree-sitter
  - `update_todo_list` — the agent maintains its own structured task list
- **Permission layer** — reads are auto-allowed, writes and shell commands require explicit approval
- **Context compression** — conversation history is compressed when it grows too large
- **Plan mode** — the agent produces a TODO list, then executes items one at a time with your confirmation between steps
- **Behavior tree mode** — experimental, forces the agent through a fixed execution sequence
- **Local model support** — runs GGUF models via llama-cpp-2 in addition to the OpenAI API
- **OpenAI tracing** — optional integration with the OpenAI tracing platform for debugging run traces

---

## Does it work?

Yes. I've used it on real tasks: a major codebase refactoring session through plan mode (12-item TODO list, approved each step), and a small chat-based LMS system I gave it as a benchmark. It built something coherent and functional.

But I want to be honest about what "works" means. It's noticeably less efficient than Claude Code, requires more supervision, and occasionally makes decisions that make me wince. For anything serious, I'd reach for Claude Code. Understanding what's inside it was the actual goal.

---

## Architecture

Layered: `domain/` → `infrastructure/` → `repository/`

```
src/
├── domain/
│   ├── tools/          # Tool implementations (find_files, read_objects, patch_files, ...)
│   ├── workflow/       # Workflow engine — orchestrates tool calls per mode
│   ├── session/        # Session lifecycle, TODO-item sub-agents
│   ├── permissions/    # Permission validation, dangerous command detection
│   ├── plan/           # Plan mode logic
│   ├── bt/             # Behavior tree mode
│   └── prompting/      # System prompts, prompt construction
├── infrastructure/
│   ├── cli/            # TUI (ratatui), REPL, input handling
│   ├── inference/      # OpenAI and local GGUF inference engines
│   ├── api_clients/    # OpenAI HTTP client (custom, not a library)
│   ├── auth/           # OAuth flow
│   ├── db/             # SQLite connection pool, schema migrations
│   └── event_bus.rs    # crossbeam channels: UiToAgentEvent / AgentToUiEvent
├── repository/         # SQLite repositories, struct-per-table
└── utils/
    └── parsing/        # Tree-sitter wrappers for 14 languages
```

The UI thread and agent thread never share state directly. Everything goes through the event bus — two unbounded crossbeam channels. The agent sends progress updates, permission requests, and results; the UI sends user input, permission decisions, and settings changes.

The OpenAI client is hand-rolled (`src/infrastructure/api_clients/`). No official Rust SDK existed at the time. The interesting file is `translator.rs` — it maps internal `ChainStep` domain types to the flat `ChatMessage` array the API expects, which is where decisions about what conversation history *means* live.

---

## Building

Requires Rust (stable). The llama-cpp-2 dependency will build the C++ library — this takes a while on the first build.

```sh
cargo build --release
```

Run:

```sh
./target/release/quillcode
```

You'll need an OpenAI API key or use OpenAI web login to save oauth token. Web search requires a Brave Search API key.

---

## Status

Side project, no roadmap, no stability guarantees. Currently only supports OpenAI models (local GGUF works but quality varies significantly by model). If you want to poke around, add things, or use it as a reference — see [CONTRIBUTING.md](CONTRIBUTING.md).
