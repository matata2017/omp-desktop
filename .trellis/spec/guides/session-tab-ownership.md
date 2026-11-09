# Session / Tab Ownership

Contracts for opening sessions, claiming transcripts, and delivering a change to the
binary the user actually runs. Learned from the 2026-11-09 sidebar parallel-tabs task.

## Ownership is claimed before the process starts

`tab:spawn` must `claim_session` **before** `start_tab`, and roll the claim back if the
spawn fails. A check-then-claim split (check owner → spawn → claim) leaves a window where
two concurrent spawns both pass the check and both start a sidecar for one transcript;
swallowing the claim error with `let _ =` hides it.

- Conflict returns `{ tabId: null, refusal: "owned", ownerTabId }`; the renderer routes to
  the owner tab instead of opening a duplicate.
- `release_process` **keeps** `session_owners`: an evicted tab stays registered and resumes
  the same transcript on focus. Releasing ownership there would let another tab steal it.
  `close` is what drops ownership.
- `sync_session_binding` may legitimately move a tab to the `sessionFile` its sidecar
  reports and release the previous binding. A log line showing two spawns with the same
  requested `sessionPath` is therefore **not** proof of an ownership bug — `tab:spawn` logs
  the *requested* path, not the settled binding. Check `tab-layout.json` and
  `tabs.getSessionOwner` before concluding.

## A tab holding a conversation is not a scratch pad

Switching a resident sidecar onto another session aborts the in-flight turn, so only a
truly blank tab may be repurposed. Pick the signal that actually reaches the renderer:

| Signal | Verdict |
|---|---|
| hydrated transcript rows + `liveMessages` | reliable; scoped store follows the focused runtime |
| `tab.sessionPath` | useful only as a pre-hydrate fallback — Rust's `tab:get-all` / `TAB_STATUS` do not send it, so restored tabs have `undefined` |
| `messageCount > 0` | unusable alone: a fresh session's `get_state` reports `1` |
| `sessionFile` from `get_state` | unusable alone: a fresh tab gets one too |

When a test harness registers no session runtime, `sessionRuntimeStore(tabId, …)` returns
`null` and any assertion built on it passes vacuously. Seed the scoped hook's fallback
store instead, and assert the restored-tab case explicitly (`sessionPath === undefined`
plus hydrated history) rather than fabricating the field.

## Deliver to the binary the user runs

Source edits and green unit tests are not delivery. For a packaged desktop app:

1. Rebuild the bundle, then install it.
2. Prove the installed artifact carries the change. A same-version NSIS `/S` run can leave
   the old exe in place; compare the installed exe against the built one, and check an
   embedded asset name (e.g. the hashed `dist/assets/index-*.js`) rather than trusting
   `INSTALL_EXIT=0`. A few bytes of difference can be the bundler's own type patch, so a
   hash mismatch alone does not prove a stale install — diff the bytes.
3. Exercise the changed path on the installed build through its real entry point. Calling
   a lower-level API that bypasses the changed function proves nothing about that function.
4. Close the app gracefully (window close / `app:quit`) so sidecars are torn down; killing
   the main process orphans them.

## Counting processes on a shared machine

Scope process checks to the artifact under test. `Get-Process omp` counts every ambient
`omp.exe` on the box — another checkout's GUI and its children included. Filter by the
installed executable path, or compare against the child PIDs captured before the action.
A machine-wide count is not a teardown check, and killing by image name destroys unrelated
user work.

## Assert the precondition, not just the outcome

A guard that returns `false` for several reasons cannot be credited to one of them from
the outcome alone. "A new tab opened" is consistent with the content guard firing **and**
with a blank tab misfiring past the reuse path. Capture the state that makes the intended
branch the attributable one: the tab was ready, not streaming, not compacting, had an empty
queue, and carried real transcript rows. Then confirm the other half of the promise — the
original tab still owns its session (`getSessionOwner`) and the clicked session is owned by
the new tab. Tie the spawn log line to the exact session path that was clicked.
