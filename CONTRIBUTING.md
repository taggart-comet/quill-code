# Contributing

QuillCode is a learning project. If you want to understand how coding agents work by reading and modifying real code, this is a good place to do it. No formal roadmap, no promises about what gets merged — but if you want to build something and learn, welcome.

---

## Setup

```sh
git clone <this-repo>
cd quill-code
cargo build
```

After every change: `cargo check --all-targets`. Never leave it not compiling.

---

## Codebase orientation

Layered architecture: `domain/` → `infrastructure/` → `repository/`. Dependencies only flow inward.

The two OS threads (UI and agent) communicate exclusively through `src/infrastructure/event_bus.rs` — two crossbeam channels, `UiToAgentEvent` and `AgentToUiEvent`. Start there if you're confused about data flow.

Read `AGENTS.md` for key files and naming conventions.

---

## Things worth adding

**Another LLM provider** — The OpenAI client is in `src/infrastructure/api_clients/openai/`. The interesting file is `translator.rs` (maps internal `ChainStep` types to the flat `ChatMessage[]` the API expects). Adding Anthropic or Gemini means new DTOs, a new translator, a new `InferenceEngine` impl, and wiring into `model_registry.rs`. The mechanical part is easy. The real work is tuning system prompts and tool descriptions for each model's behavior — that's where you'll learn most.

**A new tool** — Implement in `src/domain/tools/`, register in `src/domain/workflow/toolset/`. Write a precise description: the model calls your tool based on the description alone, not the implementation. Route filesystem/shell calls through the permission layer. Good candidates: `write_file`, filtered `list_directory`, a test runner that summarizes output.

**Better context compression** — Lives in `src/domain/workflow/workflow.rs`. Currently basic. Room to improve: smarter decisions about what to keep vs. summarize, retaining still-relevant tool results, preserving TODO state across compressions.

**Permission calibration** — `src/domain/permissions/checker.rs`. The hard problem: if every `cargo check` requires a click, users grant blanket session permission immediately. Ideas: session-learned allow-lists, distinguishing read-only shell commands from mutating ones, per-project configs.

**Diff robustness** — `patch_files` uses unified diffs the model generates; they're often malformed. Improving `patch_files` to handle common patterns (off-by-one hunk context, wrong function context) would meaningfully improve task success rates.

**Local model improvements** — Engine is in `src/infrastructure/inference/local.rs`. No prompt tuning exists for local models the way it does for OpenAI. Exploring which models work and what prompt adjustments help is open territory.

---

Keep the scope tight. A tool that works beats a partial refactor of three layers.
