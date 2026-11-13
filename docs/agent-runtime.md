# Agent runtime (`omp.exe`)

Last verified: 2026-11-12 (sidecar swap to `omp/18.19.0`). Cost/cap rows: 22:00 build; startup and open/close: 00:51 build — **all measured on `omp/18.15.0`**, before the 2026-11-12 swaps to 18.17.0 and 18.19.0.

## 1. What it is

`omp.exe` is the agent runtime Romp drives: a **fork build of the upstream omp CLI**, shipped as a resource. This repository does not compile it.

| Copy | Size | SHA256 (first 16) |
|---|---|---|
| `resources/omp.exe` | 263,930,368 B (251.7 MiB) | `4D8BC97105195358` |
| `target/release/omp.exe` (bundle copy) | same | `4D8BC97105195358` |
| `%LOCALAPPDATA%\Romp\omp.exe` (installed) | same | `4D8BC97105195358` |
| `C:\Program Files\omp\omp.exe` (host CLI; runs the assistant, not Romp) | 202,690,560 B | `2A388D8F5DE07870` |

- `--version` of the bundled copy: `omp/18.19.0` (full sha256 `4D8BC9710519535817F3D824E774FDABD729B687A3E814C7648EBF048AE20FD1`).
- `resources/omp.exe` is gitignored; the one-command upgrade is `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/update-sidecar.ps1` — it resolves the latest GUI tag, downloads the installer, extracts `$PLUGINSDIR/app-64.7z`, gates the candidate and copies `resources/omp.exe` over the three live copies (manual recipe: `docs/handoff.md`, §5 「sidecar 二进制重建」). Provenance of the 18.19.0 copy: `omp-0.9.42-portable.exe` (197,951,830 B) from nornzach/oh-my-pi-gui v0.9.42. Rollbacks on disk: the 18.17.0 copy (261,046,784 B, `0510906A0777DA0E`) at the three `*.prev` siblings, and 18.15.0 (255,728,128 B, `C968104F2A512E43`) at `.resources/app/resources/omp.exe`.
- Version deltas checked before the swap: ready frame unchanged; `get_settings_schema` / `terminal_list` / `get_available_commands` all succeed; the 109-command list is byte-identical to 18.17.0; settings entries 605 → 615 with **13 added / 3 removed / 1 default change**, not purely additive — added `browser.webkit`, `browser.webkitPath`, `claudeResets.autoRedeem{5h,Fable,Weekly}`, `codexResets.autoRedeem{5h,Weekly}`, `gc.strip`, `gc.stripAfterDays`, `launch.persist`, `systemPromptTemplates`, `task.autoParallel`, `task.steps`; removed `claudeResets.autoRedeem`, `codexResets.autoRedeem`, `task.file`; `spotlight.experiment.holdout` default 0.15 → 0.05. Our settings UI is schema-driven (`settingTranslation(...)?.label ?? entry.label ?? entry.path`), so the removed paths simply stop rendering and their `schema-zh.ts` / `schema-ru.ts` strings go unused.
- Todo statuses: `set_todos` on 18.19.0 answers `success:true` **and echoes `todoPhases` back with `"status":"blocked"`** (probe `todos-persist.ps1`, isolated root), so upstream 18.19 dropping `blocked` from its TUI did not touch the RPC surface. Residual, unexercised: hydrating a *saved* session that still contains `blocked` (the probe never opened an agent turn, so nothing reached `agent.db`; `grep -a PROBE-BLOCKED` over the temp root found no file).
- The first, wrongly unisolated 18.19.0 smoke wrote to the live `~/.omp/agent` store. Checked afterwards: `schema_version.version = 6` and 23 tables, identical to the pre-swap copy at `.perf/rpc-caps/agent.db` and to a post-run copy → **no schema migration**, WAL/session bookkeeping writes only.
- `crates/omp-agent` has no `[[bin]]` (only `examples/probe.rs` has `main`): it is the **host-side** pool/RPC/argv library, not the agent.

## 2. Cost model (Private bytes, installed build, `omp/18.15.0`)

Measured with `scripts/perf/measure.ps1` on the installed exe, isolated `OMP_DATA_DIR`:

| Condition | Sidecars | Private per sidecar | Total private |
|---|---|---|---|
| 1 active tab, empty session (N=3) | 1 | 489–506 MB (median 501) | ~828 MB (whole tree) |
| 1 active tab, resumed 6.92 MB session (N=3) | 1 | 534–542 MB (median 537) | — |
| 4 tabs, session-bound, cap 6 (N=3) | 4 | 494–511 MB (median 506) | 2328–2366 MB (median 2353) |
| 4 tabs, session-bound, cap 4 (N=3) | 4 | 502–509 MB | 2346–2359 MB (median 2347) |
| 4 tabs, session-bound, cap 3 (N=3) | 3 | 476–495 MB | 1785–1807 MB (median 1797); one tab not ready |

- Resuming a 6.92 MB transcript costs **+36 MB private** (~5× the file); the ~500 MB baseline dominates.
- WorkingSet totals are lower and not additive with Private: four session-bound tabs were RSS 1506–1525 MB vs private 2346–2366 MB. Report Private for budgets, WorkingSet only for relative comparisons.

## 3. Levers owned by this repository

- **How many run**: `ROMP_MAX_SIDECARS` (default 6, `src-tauri/src/ipc.rs::max_sidecars`), lazy start, LRU eviction of idle **session-bound** tabs, `switch_session` reuse inside a workspace.
- **When they start**: active tab only at launch; `ensure_tab_started` on activation.
- **What we send**: `--session` replay, `--no-auto-resume`, host-tool definitions, IPC batching (`batcher.rs` 32 ms).
- **GUI side**: WebView2/renderer bundle, stats lazy start, window lifecycle.

## 4. Not owned here

- In-process allocation, caches, and the ~500 MB private baseline.
- RPC protocol and event shapes (a shared-sidecar design was vetoed in `docs/architecture.md`).
- Session file format and resume validation.

## 5. Known traps

- `OMP_DATA_DIR` isolates GUI data only; sessions live under `PI_CODING_AGENT_DIR` or `~/.omp/agent`. A temp `PI_CODING_AGENT_DIR` must also carry `models.yml`/`models.db*`/`config.yml`/`agent.db*`; with only `sessions/`, resume fails with `SessionModelRestoreError` after ~200 ms.
- Fresh tabs (no session file) are **not evictable**; at the cap, `tab:spawn` returns `null` instead of recycling one.
- `tab:get-all` calls `ensure_tab_started` for the active tab, so polling `tabs.list` while a sidecar is down turns one exit into a restart storm (32 spawns / 26 exits observed). With cap 3 and four session tabs, one tab stays `starting` (median 1797 MB private vs 2347 at cap 4).
- `measure.ps1` kills the tree in `finally`; pass `-KeepRunning` when sampling again after it returns.
