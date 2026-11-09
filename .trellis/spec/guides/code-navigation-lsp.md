# Code Navigation: LSP Before Grep

> **Rule**: symbol-level questions go to the language server. Text search is for text.

## What MUST use LSP

| Question | Action |
|----------|--------|
| Where is this defined? | `definition` |
| Who uses this symbol? (required before changing/removing an export) | `references` |
| What type is this expression? | `hover` / `type_definition` |
| Who implements this trait / interface? | `implementation` |
| Does this file or glob compile clean? | `diagnostics` |
| Rename a symbol or file across the project | `rename` / `rename_file` |
| Import fix, quick fix, server-known refactor | `code_actions` |

Call shape: `xd://lsp` with `{ action, file, line, symbol }`. `line` is 1-based, `symbol` is a substring of the name on that line. `symbols` searches the workspace by name when the file is unknown.

## Why

grep misses or misreports: re-exports and aliases, trait/impl dispatch, generics, shadowing, same-named symbols in other modules, usages behind `use ... as`, and symbols referenced from string-keyed protocols. A rename done by text search silently drops callsites the compiler cannot see.

## What grep/glob remain for

- Locale strings, JSON, Markdown, config, CSS, log lines, fixtures.
- Sweeps across file types where no server knows the language (`docs/**`, `.trellis/**`).
- Proving a string is gone or appears exactly once — with an explicit expectation, never a bare count.
- Locating which file holds a symbol. Once the file is known, ask LSP the real question.

## Fallbacks MUST be declared

If no server is configured for the language, or the server errors, state it in the reply: language, action, error, and the text search used instead. NEVER present grep output as symbol-accurate.

- `rust-analyzer` serves `src-tauri/**` (`Cargo.toml`). Cold start may return `-32801 content modified`; retry with a larger `timeout`.
- `typescript-language-server` serves `src/**` and `src/shared/**`.

## Cross-layer contracts

An IPC payload shape is a symbol-level contract owned by two languages. Before changing a field: `references` on the TypeScript interface, then read the Rust producer, then diff the two shapes field by field.

`kind` vs `dir`, or `children?: FsTreeEntry[]` vs a flat list, compiles cleanly in both languages while the UI renders a flat list of file icons. TypeScript cannot see the Rust JSON, and Rust cannot see the TS interface — references plus an explicit shape diff are the only cheap oracle.
