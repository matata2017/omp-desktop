# Correctness audit — Romp e2a9289 (read-only, no build/test run)

Object: `D:/work/huanghao/omp-desktop-remote-review` @ `e2a9289`.
Every claim below is labeled **[F]** (read in source at the cited line), **[H]** (hypothesis
derived from source but not observed at runtime), **[U]** (unmeasured — no run, no test
execution in this session). No build, lint, test, formatter or app launch was performed.

## 8 verified facts (anchor set)

1. **[F]** Sidecar teardown is `start_kill()` only — no wait, no Job Object, no graceful
   `shutdown` frame: `crates/omp-agent/src/sidecar.rs:143-153` (`terminate` / `dispose`), and
   `SidecarHandle::dispose` maps to `terminate` (`crates/omp-agent/src/pool.rs:23-25`).
   Consequence: correctness of "quit leaves no orphans" rests entirely on
   `kill_on_drop(true)` (`sidecar.rs:51`) plus Tauri's own exit, not on an owned process group.
   `docs/perf.md:88-92` records that a forced parent kill already leaks children (measured
   2026-11-04), which is exactly the gap a Job Object would close.

2. **[F]** Eviction safety is a single pure predicate with 6 inputs, and it is the only guard
   between "release the process" and "lose the user's work":
   `src-tauri/src/ipc.rs:1241-1249` (`is_evictable`), fed at `ipc.rs:1270-1287`.
   Inputs: not-active, has-client, has-session, not-streaming, no pending approval,
   `browser_pages == 0`. `max_sidecars()` default is **6**, env-overridable
   (`ipc.rs:1251-1258`); `docs/perf.md:75` says 3 — doc/code drift, so any perf tuning here
   silently changes the release pressure. Any optimization that adds a lease (PTY, in-flight
   MCP/LSP, queued prompt) MUST add it to this struct *and* to the test at
   `ipc.rs:1613-1630`, otherwise the guard silently regresses.

3. **[F]** Session ownership updates require an explicit `cancelled === false`; a vetoed or
   absent flag is treated as "no switch":
   `ipc.rs:1199-1218` (`confirmed_switch_path`), test `ipc.rs:1730-1749`.
   The renderer applies the same rule independently (`use-session-switch.ts:210-219`). Two
   independent implementations of the same contract is the good case; the risk is that a perf
   shortcut (e.g. optimistic binding before the response) breaks it in Rust only and no JS test
   can see it.

4. **[F]** The renderer's tab store learns status ONLY from the `tab:status` push, and Rust emits
   it in exactly two places: `lib.rs:250` (after `ready`) and `lib.rs:397` (before spawn).
   `emit_tab_status` (`lib.rs:108-119`) has **no** `"running"` branch — it reports `ready` or
   `starting` only. Therefore `applyTabStatus`'s background-completion logic
   (`tabs.ts:777-781`, requires `previous?.status === "running"`) can only fire from the
   renderer-local promotion at `tabs.ts:394`. **[H]** Any change that stops that local promotion
   (or that debounces/coalesces `tab:status`) silently disables the unread-done badge and the
   `notifyBackgroundSettle` path (`tabs.ts:816-828`) — a user-visible regression with no test
   coverage in Rust.

5. **[F]** The in-place switch guard is a 9-condition predicate in the renderer only
   (`use-session-switch.ts:249-272`: kind, worktree, pendingSessionPath, status==="ready",
   !isStreaming, !isCompacting, !switchPending, `session.cwd === tab.cwd`, no draft/images/
   sending, no pending extension UI, no plan approval, empty steering/followUp). Rust has no
   equivalent guard: `switch_session` is forwarded verbatim (`ipc.rs:35-52`). So correctness of
   "never switch away from live work" is a *renderer* invariant; a perf change that moves
   switching into Rust (e.g. to avoid a round trip) must port all 9 conditions or the abort
   semantics (`switch_session` aborts the in-flight turn) start destroying runs.

6. **[F]** IPC contract is wider than the Rust dispatcher: 10 of 87 `IPC_COMMANDS` entries have no
   `ipc_invoke` arm — `scheduler:validate`, `scheduler:set-limits`, `sidecar:select-project`,
   `sidecar:set-project`, `fs:read-plan`, `fs:read-image`, `updater:check`, `updater:download`,
   `updater:apply`, `updater:getStatus` (diff of `src/shared/ipc-types.ts` IPC_COMMANDS vs
   `src-tauri/src/ipc.rs` match arms; only `updater:getStatus` exists). Live callers exist for
   at least `fs.readPlan` (`PlanDockCard.tsx:185`), `fs.readImage` (`markdown.tsx:569`),
   `scheduler.validate` (`TaskForm.tsx:33`), `sidecar.setProject` (`WorkspaceDialog.tsx:101`),
   and the updater buttons (`UpdateBanner.tsx:32,80,92`). All of them reject with
   `unknown channel`. **This is a pre-existing correctness hole, not a perf trade-off** — but it
   means "the tests are green" cannot be used as evidence of end-to-end correctness.

7. **[F]** Scheduler runs are the least-guarded child processes: `run-now` spawns with
   `stderr(Stdio::null())`, **no `kill_on_drop`**, and the only reaper is `supervise_run`
   (`ipc.rs:920-1002`; `scheduler.rs:256-320`). `SchedulerService::reap()` (`scheduler.rs:164`)
   has **no caller** in `src-tauri` (grep: only its own test at `scheduler.rs:453`), and
   `AppState.scheduler_runs` is not touched by `app:quit` (`ipc.rs:397-411`) or
   `on_window_event` (`lib.rs:958-971`) — both stop bench, browser, tabs, stats only.
   **[H]** A live scheduled run therefore survives quit as an orphan, and its `runs` row stays
   `running` forever. Also `scheduler:get-snapshot` reports `limits.maxConcurrent = 2` as a
   constant (`ipc.rs:748`) while `scheduler:set-limits` does not exist — the UI's limit control
   is inert.

8. **[F]** Stats dashboard and bench both hold a `Child` behind `parking_lot::Mutex` and are
   killed with `start_kill()` without `wait()` (`crates/omp-stats/src/lib.rs:227-243`,
   `bench.rs:86-92`); `ensure_running` has a *single-flight-free* race: two concurrent callers
   both pass `child_alive()` and both spawn a server (`omp-stats/src/lib.rs:160-212`), with the
   later `*self.child.lock() = Some(child)` overwriting the first handle. **[H]** A perf change
   that makes stats reads concurrent (e.g. prefetching several endpoints in parallel on boot)
   turns a 96 MB singleton into N dashboards whose earlier instances are unreachable-but-alive.

## User-observable contracts, by risk

Ordered by "how bad is it if an optimization breaks it" × "how invisible is the breakage".

| # | Contract | Anchor | What an optimization would break |
|---|---|---|---|
| R1 | Closing/quit never loses unsaved session state and never orphans children | `sidecar.rs:143-153`, `lib.rs:958-971`, `ipc.rs:397-411` | Job-object or "detach and let the OS clean up" shortcuts; async dispose that returns before the child is gone |
| R2 | A released sidecar can always be resumed on the exact session it held | `ipc.rs:1241-1249`, `sync_session_binding` `ipc.rs:1167-1196` | Binding written from `get_state` *before* success is confirmed; eviction before the first `get_state` |
| R3 | Switching session never aborts a live turn or drops a queued prompt | `use-session-switch.ts:249-272` | Moving the guard to Rust partially; skipping the `cwd`/kind comparison to avoid a `get_state` |
| R4 | Background tabs still notify and show "done", and their transcript is not stale | `tabs.ts:394,777-781,816-828` | Coalescing `tab:status`; dropping the renderer-local `running` promotion; skipping `replay_ready` (`ipc.rs:1154-1164`) |
| R5 | Tab layout survives restart, including which tab was active | `tab_layout.rs:56-78` (atomic tmp+rename), `lib.rs:722-758` | Writing layout from a background thread without the rename; persisting only on `tab:spawn` (dropping `tab:set-active`/`set-view` at `ipc.rs:223,253`) |
| R6 | A session file is attached to at most one sidecar | `pool.rs:213-235` (`claim_session`), `ipc.rs:152-158` | Pre-checking ownership only in the renderer (already best-effort there, `use-session-switch.ts:170-175`); caching the owner map |
| R7 | Scheduled runs are isolated, non-escalating and eventually terminate | `scheduler.rs:30-93` (denylist), `run-argv` test `scheduler.rs:397-440` | Reusing a warm sidecar for runs to save a spawn; passing user flags through |
| R8 | Renderer/event shapes stay exactly as `omp-api.ts` expects | `lib.rs:170-232` (3 payload shapes), `lib.rs:1050-1096` tests | Batching dedicated channels into `rpc:events` (terminal frames, extension UI, host tools each have their own shape) |

## What independent evidence would prove an optimization kept correctness

The current gates cannot do this: 158 Rust `#[test]` fns and 187 renderer test files exercise
*pure functions* (argv, cron, store CRUD, guards) — `cargo test`/`vitest` never spawn the real
`omp.exe`, never run two tabs, and never kill a parent. "Tests pass + AI says it looks fine" is
therefore structurally insufficient. Minimum independent evidence, in increasing cost:

1. **Process-tree audit (no app changes).** A harness that, per scenario, snapshots
   `ParentProcessId` descendants of `romp-desktop` before/after: open 4 tabs → quit; open 2 tabs →
   force-kill parent (`Stop-Process`) → assert zero `omp.exe` survivors (today it fails by
   design, `docs/perf.md:88`); trigger `scheduler:run-now` → quit → assert no survivor. This is
   the only way to prove R1/R7; `kill_on_drop` cannot be asserted from unit tests.
2. **Sidecar-level protocol probe as a differential oracle.** `crates/omp-agent/examples/probe.rs`
   already drives the real binary. Extend it to assert the two contracts Rust *assumes*:
   `switch_session` returns `data.cancelled === false` for same-cwd and `true` for cross-cwd, and
   `get_state.sessionFile` is present on a fresh tab. Run it before/after the optimization
   against the same binary; a behavior change then shows up as a diff, not as a test rewrite.
3. **Golden-trace replay of the IPC surface.** Capture `runtime.log` + every emitted
   `(channel, payload)` for a scripted session (open tab → switch session → background turn →
   close), then replay the *recorded sidecar frames* through `route_frame` in a test. This pins
   R4/R8 (payload shapes and `tab:status` cadence) without launching the GUI, and it fails loudly
   if a perf change coalesces or reorders frames.
4. **Crash-consistency check of persistence.** Kill the app mid-`persist_layout`
   (`tab_layout.rs:70-77`) N times and assert `tab-layout.json` is either old-and-valid or
   new-and-valid (never truncated). Same for `prefs.json` (`ipc.rs:1479-1489`).
5. **Cross-process invariant assertion in the live app** (cheapest to keep running): a debug-only
   command that, for every tab, asserts `tab_sessions[tab]` exists whenever
   `pool.is_started(tab)`, and that `session_owners` has no entry whose tab is absent. Run it on a
   CDP script after each scenario; it catches R2/R6 drift that no unit test can see.

## Explicitly unmeasured

- **[U]** No scenario was executed; every failure mode above is a source reading plus recorded
  history (`docs/perf.md`), not a fresh observation.
- **[U]** Whether `kill_on_drop` actually fires on Tauri `app.exit(0)`/window close on this
  machine (the code path is `on_window_event` → `pool.close` → `start_kill`, so it *should*, but
  nothing in the repo asserts it).
- **[U]** Whether a live scheduler run survives quit (fact 7 is the missing teardown, the leak
  itself is unobserved).
- **[U]** Release-build numbers; all recorded timings are dev-build (`docs/perf.md:3-8`).
