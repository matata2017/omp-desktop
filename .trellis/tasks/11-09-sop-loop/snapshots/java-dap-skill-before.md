---
name: omp-java-dap-debug
description: "Set up and run Java breakpoint debugging through OMP's debug (DAP) tool on Windows with kotlin-debug-adapter, including the configurationDone ordering proxy that prevents launch/attach timeouts."
---

# OMP Java DAP debugging (Windows, JDK 17)

OMP's `debug` tool has no built-in Java adapter. `kotlin-debug-adapter` works, but only behind a small ordering proxy — without it every launch/attach times out.

## Why the proxy exists

`kotlin-debug-adapter` emits `initialized` immediately after `initialize`, then **blocks its `launch`/`attach` response until it receives `configurationDone`**. OMP waits for the launch/attach response *before* sending `configurationDone`. Both sides wait → `DAP launch failed: Internal error` / `DAP configurationDone also failed: The operation timed out.`

The proxy answers `configurationDone` on OMP's behalf the moment OMP forwards `launch`/`attach` (the adapter is already `initialized`), and answers OMP's own later `configurationDone` locally so the adapter sees exactly one.

## Install (once)

1. Adapter distribution: `https://github.com/fwcd/kotlin-debug-adapter/releases/download/0.4.4/adapter.zip` → extract to `%LOCALAPPDATA%\kotlin-debug-adapter\adapter` (22 jars under `lib/`, main class `org.javacs.ktda.KDAMainKt`). Do **not** use its `bin/kotlin-debug-adapter.bat` from OMP: batch files cannot be spawned without a shell.
2. `~/.omp/agent/dap/adapter.json`: `{"command": ["D:/jdk17/bin/java.exe", "-classpath", "<lib jars joined by ; with forward slashes>", "org.javacs.ktda.KDAMainKt"]}`. Generate the classpath with a script — hand-typing loses slashes (`C:Users/...` becomes drive-relative and only works when CWD is on `C:`).
3. `~/.omp/agent/dap/dap-ordering-proxy.js`: reads `ADAPTER_CMD` (JSON array) or `adapter.json`, spawns the adapter, frames Content-Length messages both ways, injects `configurationDone`.

## Wire it

Project-level `<repo>/.omp/dap.json` (overrides the user-level entry):

```json
{"adapters": {"custom-jvm": {
  "command": "node",
  "args": ["C:/Users/<user>/.omp/agent/dap/dap-ordering-proxy.js"],
  "languages": ["java"], "fileTypes": [".java"], "rootMarkers": ["pom.xml"],
  "launchDefaults": {"request": "launch", "mainClass": "<main class>", "projectRoot": "<module dir>"},
  "attachDefaults": {"request": "attach", "projectRoot": "<module dir>", "hostName": "127.0.0.1", "timeout": 5000}
}}}
```

## Run it — prefer attach

Attach skips classpath resolution (the adapter would otherwise shell out to `mvn dependency:list`, which stalls without the internal Nexus).

```bash
mvn spring-boot:run -Dspring-boot.run.jvmArguments="-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=5005"
```

Then: `debug` attach (`adapter: "custom-jvm"`, `port: 5005`) → `set_breakpoint` (file + line) → `continue` → `evaluate` / `variables`.

For a repo whose Spring context cannot start (no Redis/DB), `suspend=y` plus a breakpoint on `main` proves the chain with no dependencies.

## Verification

- `debug` `sessions` returns a session with `Status: running`.
- `set_breakpoint` reports `verified`.
- `continue` reports `Stop reason: breakpoint` and a `Location`.
- `evaluate` returns a real runtime value (e.g. a loop counter), not `unknown`.

## Pitfalls

- One root session at a time: `terminate` before re-attaching, else `Debug session ... is still active`.
- Breakpoint source needs a file name; a path-only `source` NPEs inside the adapter (OMP fills `name`, so the tool path is fine).
- `evaluate` with string concatenation may return `unknown`; evaluate single identifiers.
- Adapter on a dead port logs `Connection refused` and the session ends immediately — check `netstat -ano | grep :5005` first.
- Killing the debuggee leaves a stale session; `debug` `terminate` then re-attach.
- `kotlin-debug-adapter` launch mode calls `mvn dependency:list` without `-o`; on a repo needing an internal Nexus, pre-warm the local `.m2` or stay on attach.

## OpenIDE (D:\openIde) is a different story

Its Java Debugger needs Java 21+: the managed jdtls wants JDK 25, and Microsoft `java-debug` 0.53.2 declares `Require-Capability: osgi.ee ... version=21`, so a custom JDK-17 jdtls entry gives working navigation but `vscode.java.startDebugSession` returns `-32601 No delegateCommandHandler`. Custom servers in `%APPDATA%\ai.l0ng.openidea\settings.toml` use snake_case keys (`root_markers`, `arguments` as one string), `command` must be a directly spawnable executable (a `.cmd` shows "Missing"), and the app rewrites that file on save, eating `/` in paths.
