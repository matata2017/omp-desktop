# Agent runtime (`omp.exe`)

Last verified: 2026-11-09. Cost/cap rows: 22:00 build; startup and open/close: 00:51 build.

## 1. What it is

`omp.exe` is the agent runtime Romp drives: a **fork build of the upstream omp CLI**, shipped as a resource. This repository does not compile it.

| Copy | Size | SHA256 (first 16) |
|---|---|---|
| `resources/omp.exe` | 255,728,128 B (243.9 MiB) | `C968104F2A512E43` |
| `target/release/omp.exe` (bundle copy) | same | `C968104F2A512E43` |
| `%LOCALAPPDATA%\Romp\omp.exe` (installed) | same | `C968104F2A512E43` |
| `C:\Program Files\omp\omp.exe` (host CLI; runs the assistant, not Romp) | 202,690,560 B | `2A388D8F5DE07870` |

- `--version` of the bundled copy: `omp/18.15.0`.
- `resources/omp.exe` is gitignored; rebuild = extract `$PLUGINSDIR/app-64.7z` from the upstream v0.9.35 portable (see `docs/handoff.md` §8).
- `crates/omp-agent` has no `[[bin]]` (only `examples/probe.rs` has `main`): it is the **host-side** pool/RPC/argv library, not the agent.

## 2. Cost model (Private bytes, installed build)

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
