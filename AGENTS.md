<!-- TRELLIS:START -->
# Trellis Instructions

These instructions are for AI assistants working in this project.

This project is managed by Trellis. The working knowledge you need lives under `.trellis/`:

- `.trellis/workflow.md` — development phases, when to create tasks, skill routing
- `.trellis/spec/` — package- and layer-scoped coding guidelines (read before writing code in a given layer)
- `.trellis/workspace/` — per-developer journals and session traces
- `.trellis/tasks/` — active and archived tasks (PRDs, research, jsonl context)

If a Trellis command is available on your platform (e.g. `/trellis:finish-work`, `/trellis:continue`), prefer it over manual steps. Not every platform exposes every command.

If you're using Codex or another agent-capable tool, additional project-scoped helpers may live in:
- `.agents/skills/` — reusable Trellis skills
- `.codex/agents/` — optional custom subagents

Managed by Trellis. Edits outside this block are preserved; edits inside may be overwritten by a future `trellis update`.

<!-- TRELLIS:END -->

## Code navigation: LSP first, text search second

Symbol-level questions MUST go through the language server (`xd://lsp`), never grep:

- where a symbol is defined, who references it, who implements it, what type an expression has, diagnostics, cross-file renames, import fixes. `references` is mandatory before changing or removing an exported symbol.
- grep/glob stay for non-symbol text: locale strings, JSON, Markdown, config, CSS, log lines, fixtures — and for locating the file that holds a symbol. Once the file is known, the actual question goes to LSP.
- No server for the language, or an LSP error → say so in the reply (language, action, error) and name the fallback used. NEVER present grep output as symbol-accurate.
- Verified in this repo: `rust-analyzer` serves `src-tauri/**`, `typescript-language-server` serves `src/**`; a cold-start `-32801 content modified` is retryable, not a reason to fall back.
