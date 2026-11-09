---
name: omp-lsp-windows
description: "Wire Eclipse JDT LS / pyright / gopls into omp on Windows with absolute commands, including the m2e classpath-rewrite workaround and Lombok javaagent"
---

# omp LSP wiring on Windows

Use when setting up or repairing omp language servers (`lsp status` shows nothing, or files report "not on the classpath ... only syntax errors are reported").

## Rules that prevent most failures

1. **Never use `.cmd`/`.bat` shims as `command`** — omp spawns without a shell. Eclipse's `jdtls.bat` is `python %~dp0/jdtls %*` + `pause` (hangs). pyright's `pyright-langserver.cmd` is a forwarding shim; prefer `node.exe` + the entry js.
2. **Override `command` with an absolute path**; omp's built-in entries assume a binary on PATH.
3. Overriding `args` replaces the whole array — pass the complete argument list.
4. Servers only auto-start when **cwd** contains their `rootMarkers` (cwd-only, no parent walk). A Java repo will never start pyright/gopls.
5. Verify with a real request, not just `status`: `lsp hover`, `lsp symbols`, `lsp diagnostics`.

## Java (Eclipse JDT LS)

1. Pick the version for the installed JDK: 1.40.x needs Java 17, 1.44+ needs 21, current snapshots need 25.
2. Directory listings are blocked by a WAF unless you add `?d` — `https://download.eclipse.org/jdtls/milestones/<ver>/?d` (301 → follow `-L`) lists `jdt-language-server-<ver>-<timestamp>.tar.gz`. Download that exact URL plus `.sha256`, verify, `tar -xzf` into `%LOCALAPPDATA%\jdtls`.
3. `bin/jdtls.bat` is unusable; launch the launcher jar directly:

```json
"jdtls": {
  "command": "D:/jdk17/bin/java.exe",
  "args": ["-Declipse.application=org.eclipse.jdt.ls.core.id1",
           "-Dosgi.bundles.defaultStartLevel=4",
           "-Declipse.product=org.eclipse.jdt.ls.core.product",
           "-Xmx2G", "--add-modules=ALL-SYSTEM",
           "-javaagent:<lombok.jar>",
           "-jar", "<jdtls>/plugins/org.eclipse.equinox.launcher_<v>.jar",
           "-configuration", "<jdtls>/config_win",
           "-data", "%LOCALAPPDATA%/jdtls-workspace"]
}
```

4. Lombok: it is `provided`, so `dependency:copy-dependencies` never copies it. Resolve the exact version with `mvn -o dependency:tree | grep lombok` and add `-javaagent:` for it. Without it, diagnostics show fake errors (`blank final field may not have been initialized`, `setId undefined`).
5. Dependencies: `mvn -o -DskipTests -pl <module> -am dependency:copy-dependencies -DoutputDirectory=%LOCALAPPDATA%\jdtls-libs -DincludeScope=runtime`.

### The m2e classpath trap (most common breakage)

Symptom: every file warns `X is not on the classpath of project Y, only syntax errors are reported`; jdtls log shows `src/main/java/org/<pkg> [in <project>] does not exist`; `definition` on a jar type returns `[]`.

Cause: m2e writes `excluding="**"` (exclude everything) onto source entries — typically when a POM declares `src/main/java` as both a source root **and** a `<resource>` directory. It rewrites `.classpath` on every import, so hand patches vanish.

Working fix (gitignored files only, no POM change):

1. Rewrite each module's `.classpath` with plain source entries:

```xml
<classpathentry kind="src" path="src/main/java"/>
<classpathentry kind="src" path="src/main/resources"/>
<classpathentry kind="src" output="target/test-classes" path="src/test/java"><attributes><attribute name="test" value="true"/></attributes></classpathentry>
<classpathentry kind="src" output="target/test-classes" path="src/test/resources"><attributes><attribute name="test" value="true"/></attributes></classpathentry>
<classpathentry kind="con" path="org.eclipse.jdt.launching.JRE_CONTAINER"/>
<classpathentry kind="output" path="target/classes"/>
```

2. Append one `<classpathentry kind="lib" path="<abs jar>"/>` per jar (generate with `ls` → absolute paths; never double-prefix the directory).
3. `attrib +R` all module `.classpath` files so m2e cannot overwrite them.
4. Hard restart: kill the `equinox.launcher` java process (`wmic process where "name='java.exe' and CommandLine like '%equinox.launcher%'" get ProcessId` → `taskkill /F /PID`), delete `%LOCALAPPDATA%\jdtls-workspace`, then `lsp reload *`.
5. Judge by `lsp diagnostics` on a Lombok-heavy class: zero errors, only warnings.

Do NOT chase `java.import.maven.enabled:false` or `java.project.referencedLibraries` — omp does send `{settings:{...}}` but jdtls imports Maven during initialize, before settings apply; the read-only `.classpath` is the only lever that held.

## Python (pyright)

```bash
npm install -g pyright --registry=https://registry.npmmirror.com   # npmjs handshake often fails
```

```json
"pyright": { "command": "C:/Program Files/nodejs/node.exe",
             "args": ["C:/Program Files/nodejs/node_modules/pyright/langserver.index.js", "--stdio"] }
```

## Go (gopls)

```bash
export GOPROXY=https://goproxy.cn,direct GOBIN="$LOCALAPPDATA/go-bin"
"$LOCALAPPDATA/go-<ver>/go/bin/go.exe" install golang.org/x/tools/gopls@latest
```

Add `%LOCALAPPDATA%\go-<ver>\go\bin` and `%LOCALAPPDATA%\go-bin` to the **user** PATH with PowerShell `[Environment]::SetEnvironmentVariable('Path', ..., 'User')` using **expanded** paths (never literal `%LOCALAPPDATA%`). gopls shells out to `go`, so the running omp process needs a restart to inherit it.

## Verification without a matching repo

Root markers gate startup, so use a throwaway dir with `go.mod` / `pyproject.toml`. Prove the server itself with a minimal stdio `initialize` handshake: LSP requires `Content-Length: N\r\n\r\n` framing (bare JSON lines hang), and the response must be read by `id`, skipping notifications.
