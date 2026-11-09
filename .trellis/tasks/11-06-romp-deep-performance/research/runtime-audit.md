# Runtime audit — sidecar spawn/reclaim, tab & session ownership, leases, copies

Snapshot audited: `D:/work/huanghao/omp-desktop-remote-review` @ `e2a92895857fbfd1fc42e34f1770a32cbffb09cd`
(independent checkout; main worktree is an older, dirty commit). Read-only audit: no build, no test,
no app launch. Line anchors are from that checkout.

Labels: **[F]** read directly in source · **[H]** hypothesis from source, not measured · **[U]** unmeasured.

---

## F1. Spawn has no in-flight guard; a historical log suggests a duplicate but is not pinned to this build

[F] `ensure_tab_started` only consults `pool.is_started` (`src-tauri/src/lib.rs:521`); the actual
`start_tab` (`src-tauri/src/lib.rs:393`, attach at `:433`) is also reachable from `tab:spawn`
(`src-tauri/src/ipc.rs:186`) with no per-tab mutex and no "spawning" slot state (`pool.rs:24-27` only
has `handle`/`started`). Two concurrent calls for the same tab therefore both see `started == false`.

[H, historical/other-build evidence — not pinned to e2a9289] A retained `%APPDATA%/dev.romp.desktop/runtime.log` shows `spawning sidecar tab=tab-1` at `08:25:26.615`, `08:28:17.515`, `08:41:30.911`, while `romp run() start` appears only at `08:25` and `08:41`. Between 08:25 and 08:41 there is no `released idle sidecar` line in `runtime.log` or `runtime.prev.log`; in this checkout the only release site is the eviction sweep (`src-tauri/src/ipc.rs:1304`). The 08:28 respawn is therefore consistent with a duplicate spawn or an unrecorded kill, but the log's binary/build hash and source commit are unknown. It cannot be treated as a runtime fact about e2a9289.

[H] If a duplicate occurs, each extra agent process has the marginal cost reported historically in `docs/perf.md`, but no current-build measurement is available here. Overturning questions: reproduce with a pinned release/debug binary on e2a9289; verify whether the second call originates from `tab:spawn` or `ensure_tab_started`; check `already_started` on the second call; verify a matching reclaim record in a fresh, per-run log.

## F2. Reclaim is reactive and conditional — never time-based, and unreclaimable tabs can wedge the cap

[F] `evict_idle_sidecar` is called from exactly two places, both on the *spawn* path:
`src-tauri/src/ipc.rs:138` (`tab:spawn`) and `src-tauri/src/lib.rs:525` (`ensure_tab_started`). There is
no timer, no idle deadline, no `tokio::time::interval` sweep anywhere in `src-tauri/src` or `crates/`.

[F] Cap is 6 by default (`src-tauri/src/ipc.rs:1251-1255`, `ROMP_MAX_SIDECARS`), while `docs/perf.md:72`
and `docs/parity.md:210` both claim 3 — documentation drift on the single most load-bearing constant.

[F] Eviction skips any tab without a session file: `src-tauri/src/ipc.rs:1268`
(`!sessions.contains_key(&tab) → continue`), and `is_evictable` requires `has_session`
(`:1241-1246`). A fresh, never-prompted tab therefore can never be reclaimed, yet it is counted by
`state.clients.lock().len()` at `:138`/`:524`. Enough such tabs and `tab:spawn` returns `Value::Null`
(`:139-140`) with no user-visible reason.

## F3. Session ownership is claimed on spawn and only re-synced on two RPC shapes

[F] `tab:spawn` claims the session after spawning (`src-tauri/src/ipc.rs:190-192`) and pre-checks the
owner map (`:151-156`); the map lives in `pool.session_owners` (`crates/omp-agent/src/pool.rs:235-254`).
`sync_session_binding` (`src-tauri/src/ipc.rs:1167-1197`) updates it only for `switch_session` with
`success:true && cancelled:false` (`:1201-1220`) and `get_state` with `data.sessionFile`. Any other
path that changes the sidecar's session (e.g. `drop_session`, `fork`, `handoff`, `new_session`) leaves
`tab_sessions` stale, and a stale binding is what `release_process`/resume then uses.

[H] Stale binding consequences: a released tab resumes the *previous* session, and a second tab can
then double-claim the real one (the ownership guard at `:151-156` reads a map that no longer matches
reality). Overturning question: after `drop_session` + `agent_end`, does `get_state.sessionFile` differ
from `tab_sessions[tab]`, and does a follow-up `tab:spawn` on that file get refused?

## F4. `sidecar:restart` does not restart anything, and no code ever notices a dead child

[F] `src-tauri/src/ipc.rs:280-286`: `sidecar:restart` iterates clients calling `reject_all("sidecar
restarting")` and clears `ready_tabs`, then returns `{ok:true}`. No kill, no spawn, no status emit.
The renderer's SidecarBanner calls exactly this as its recovery action
(`src/renderer/components/layout/SidecarBanner.tsx:78`), and `SettingsWindow.tsx:492` passes a
`{tabId, sessionPath}` payload that main ignores entirely.

[F] The stdout pump's result is discarded (`crates/omp-agent/src/sidecar.rs:98-101`
`let _ = omp_rpc::pump(...)`), and nothing calls `Sidecar::try_wait` (`sidecar.rs:134-140`) except
`omp-stats`/`bench`/`scheduler` for *their own* children. Grepping the whole `src-tauri/src` tree finds
no `"exited"` or `"restarting"` status emission, yet the renderer branches on both
(`src/renderer/hooks/use-rpc-events.ts:726`) and `RestartPolicy` (`sidecar.rs:158-192`) is exported but
never constructed outside tests. A crashed agent leaves the tab reporting `ready` with a dead client
and a wedged `RpcClient::pending` map until some unrelated spawn evicts it.

## F5. Every frame is cloned at least twice, and background tabs' frames cross the bridge too

[F] Copy chain for one inbound frame: `pump` parses JSON into `Value`
(`crates/omp-rpc/src/transport.rs:76-93`) → `route_frame(frame: Value)` by value
(`src-tauri/src/lib.rs:236`) → `EventBatcher::push(frame)` moved, then flush hands out the whole
`Vec<Value>` (`crates/omp-agent/src/batcher.rs:61-98`) → `json!({"tabId", "payload": batch})`
(`src-tauri/src/lib.rs:377-382`) → Tauri `emit` serializes again. Outbound commands are cloned twice
more: `RpcClient::command` writes `id` into a caller-supplied `Value`
(`crates/omp-rpc/src/client.rs:98-100`), then `Sidecar::send` clones it into the mpsc channel
(`crates/omp-agent/src/sidecar.rs:120-123`), and `frame_sink` clones again (`:127-131`).
Inbound responses are cloned a third time at `client.rs:127`.

[F] `pool.forwards_full` (`crates/omp-agent/src/pool.rs:191-197`) — the intended "only visible tabs
get the full stream" gate — has **zero callers**. `route_frame` batches every frame for every started
tab, and the drop happens in the renderer instead (`src/bridge/omp-api.ts:176`: envelope ignored when
`tabId !== activeTabId`). So N background sidecars each pay full IPC + JSON serialization into a
WebView that discards it.

[H] Batch interval is 32 ms (`crates/omp-agent/src/batcher.rs:8`) and the buffer only backpressures
`tool_execution_update` at 1000 entries (`:69-72`), so a 4-tab window can push ~N×30 envelopes/s
through the bridge during concurrent turns.

## F6. One tab activation costs ≥12 sidecar round trips, from several independent triggers

[F] `hydrateTabSession` (`src/renderer/hooks/session-hydration.ts:351`) fires `get_state` +
paged transcript (`:379-381`), `get_subagents` (`:384`), then a 7-way `Promise.allSettled`
(`:387-405`: `get_goal`, `get_loop_mode`, `get_vibe_mode`, `get_config_warnings`, `get_advisor_status`,
`get_queue`, `get_settings`×2) — plus `set_subagent_subscription` (`:415`). Transcript paging is
256 messages per page and loops to completion (`src/shared/rpc-client.ts:52`, `:68-90`), with a
fallback to unbounded legacy `get_transcript` if any page disagrees (`:71`, `:80`, `:86`).

[F] Hydrate is triggered from at least five places for one activation:
`src/renderer/hooks/use-rpc-events.ts:703` (ready path), `src/renderer/stores/tabs.ts:1016`
(boot fallback), `:1048` (ready transition), `:439` (showView), `:830` (session change). None of them
dedupes by in-flight generation, so a boot where `tab:status` ready, GET_TABS fallback and showView all
land in the same tick can hydrate the same tab concurrently.

[H] The 2–4 s documented "spawn → first paint" (`docs/perf.md:24-27`) is therefore split between
~0.6–0.95 s sidecar boot and this fan-out, not between spawn and handshake. Overturning question:
count RPC frames per activation in a sidecar trace — if the count is ~12 with serialized latency,
batching/removal of the secondary reads is a real win; if it is dominated by transcript bytes, the win
is in paging/merging instead.

## F7. Release is "safe" by session presence only — no lease for terminals, jobs, queues, or scheduler runs

[F] `EvictionFacts` (`src-tauri/src/ipc.rs:1228-1238`) covers exactly five facts: active, has_client,
has_session, streaming, pending_approval, browser_pages. `is_evictable` (`:1241-1246`) is the whole
policy. There is **no** lease for live PTY sessions (`terminal_open` frames are only routed, see
`src-tauri/src/lib.rs:187-192`), async jobs, queued steer/follow-up prompts, or in-flight MCP/LSP calls.
`blocking_requests` only tracks the 7 blocking UI methods (`lib.rs:702-712`).

[F] `release_process` (`crates/omp-agent/src/pool.rs:167-184`) disposes the handle and sets
`started=false` but leaves `tab_sessions`, `tab_cwds`, `tab_kinds` intact, and the caller then emits
`status:"starting"` (`src-tauri/src/ipc.rs:1311-1315`) and clears `ready_tabs` — which makes the
renderer reset every per-tab store (`src/renderer/hooks/use-rpc-events.ts:642-660`). Resume then
re-runs the whole session file through the sidecar.

[U] Whether resume after eviction costs as much as a cold start (JSONL replay + context rebuild) is
unmeasured; `docs/perf.md` only compares "1 tab vs 4 tabs", never "evicted and resumed tab".

## F8. Two extra always-on consumers: the stats dashboard, and one full sidecar per scheduled run

[F] The stats dashboard is spawned during `setup` (`src-tauri/src/lib.rs:939-945`) via
`StatsServer::ensure_running` (`crates/omp-stats/src/lib.rs:163-224`) and only stopped by
`terminate_now` (`:224-231`); `docs/perf.md:37` measures it at 96 MB. It is a second `omp.exe`
process with its own restart budget (`:99`), not the sidecar, and nothing ever idles it out.

[F] `scheduler:run-now` (`src-tauri/src/ipc.rs:920-1010`) spawns a *separate* `omp.exe`
(`:951-968`) with its own `--session-dir` (`src-tauri/src/scheduler.rs:71-92`), supervised by
`supervise_run` (`:256`). It is not counted against `max_sidecars()` and not part of `pool`, so a
scheduled run plus 6 tabs means 7 agent processes.

[F] `spawn_spare` (`src-tauri/src/lib.rs:439`) is annotated `#[allow(dead_code)]` with "boot is
disabled" (`:950-953`), and `AppState.spare` (`:72`) is never read outside that function. Any plan that
assumes a warm spare already exists is wrong; it would have to be re-enabled *and* fixed.

---

## Verification questions that could overturn candidate optimizations

1. **Duplicate spawn (F1):** in an untruncated `runtime.log`, does the 08:28 `tab-1` spawn carry
   `origin="tab:spawn"` (renderer double-click / reconcile race) or `ensure_tab_started` with
   `already_started=false` while a process was already attached? If the former, the fix belongs in
   renderer spawn dedupe, not in a Rust mutex.
2. **Copy cost (F5):** measure bytes × count on the bridge for one streaming turn in 4 tabs. If the
   bridge is not the bottleneck (WebView parse dominates), wiring `forwards_full` saves CPU but not
   perceived latency.
3. **Hydrate fan-out (F6):** instrument sidecar frames per activation. If transcript bytes dominate,
   cutting the 7 secondary reads is noise; if round-trip count dominates, it is the win.
4. **Resume cost (F7):** time `evict → ensure_tab_started → ready` and compare with a cold spawn on the
   same session. If resume ≈ cold, eviction is not a memory lever for hot sessions, only for cold ones.
5. **Eviction reachability (F2):** with 7 fresh tabs (no prompts), does `tab:spawn` return null?
   If yes, the cap must not count session-less tabs, or `is_evictable` must allow releasing them.
6. **`sidecar:restart` (F4):** trigger a banner restart and observe whether `runtime.log` records any
   spawn. If none, "Restart" is a placebo and the dead-child path is unrecoverable — which changes the
   priority of adding child-exit supervision entirely.
7. **Cap constant (F2):** is 6 or 3 the intended value? Any memory budget computed from `docs/perf.md:72`
   is built on the wrong number.
