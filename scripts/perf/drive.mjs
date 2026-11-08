// Drive the running Romp window over CDP for measurement.
// Opens tabs, boots their sidecars, and reads renderer performance marks.
// Usage: node scripts/perf/drive.mjs [--port 9222] [--open 2] [--wait-ready 30000] [--out file]
// Output: one JSON object on stdout. Exit 1 when no page target or CDP fails.
import { writeFileSync } from "node:fs";

const args = new Map();
for (let i = 2; i < process.argv.length; i++) {
	const key = process.argv[i].replace(/^--/, "");
	const next = process.argv[i + 1];
	if (next && !next.startsWith("--")) {
		args.set(key, next);
		i++;
	} else {
		args.set(key, "true");
	}
}
const port = Number(args.get("port") ?? 9222);
const open = Number(args.get("open") ?? 0);
const waitReady = Number(args.get("wait-ready") ?? 30000);
const wantedMarks = (args.get("marks") ?? "boot:first-paint,chat:first-session-rendered")
	.split(",")
	.filter(Boolean);
const close = args.get("close") === "true";

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

async function attempt() {
	// The window handle exists before WebView2 opens the debugging port. Retry discovery for
	// the same budget as the rest of the drive step.
	const fetchDeadline = Date.now() + waitReady;
	let targets;
	for (;;) {
		try {
			targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
			break;
		} catch (error) {
			if (Date.now() > fetchDeadline) throw error;
			await sleep(200);
		}
	}
	// A tab with the Browser panel open adds a second page target. The app page is the
	// dev-server URL in dev builds, or a tauri:// page in a packaged build.
	const target =
		targets.find(item => item.type === "page" && item.url.includes("localhost:1420")) ??
		targets.find(item => item.type === "page" && item.url.includes("tauri.localhost")) ??
		targets.find(item => item.type === "page" && !item.url.startsWith("http")) ??
		targets.find(item => item.type === "page");
	if (!target) throw new Error("no page target");

	const socket = new WebSocket(target.webSocketDebuggerUrl);
	let nextId = 1;
	const pending = new Map();
	socket.addEventListener("message", event => {
		const message = JSON.parse(event.data);
		const resolver = pending.get(message.id);
		if (resolver) {
			pending.delete(message.id);
			resolver(message);
		}
	});
	socket.addEventListener("close", () => {
		for (const [id, resolver] of pending) {
			pending.delete(id);
			resolver({ __error: "closed" });
		}
	});
	const send = (method, params) =>
		new Promise(resolve => {
			const id = nextId++;
			const timer = setTimeout(() => {
				if (pending.delete(id)) resolve({ __error: "timeout" });
			}, 10000);
			pending.set(id, message => {
				clearTimeout(timer);
				resolve(message);
			});
			socket.send(JSON.stringify({ id, method, params }));
		});

	await new Promise((resolve, reject) => {
		socket.addEventListener("open", resolve);
		socket.addEventListener("error", reject);
	});

	// Probe until the renderer has installed window.omp. A navigation during startup closes
	// the socket; surface that as an error so the caller reconnects instead of hanging.
	const bridgeDeadline = Date.now() + waitReady;
	let bridgeReady = false;
	while (Date.now() < bridgeDeadline) {
		const probe = await send("Runtime.evaluate", {
			expression: "Boolean(window.omp && window.omp.tabs)",
			returnByValue: true,
		});
		if (probe.__error) throw new Error(`cdp ${probe.__error} while probing window.omp`);
		if (probe.result?.result?.value === true) {
			bridgeReady = true;
			break;
		}
		await sleep(200);
	}
	if (!bridgeReady) throw new Error("window.omp bridge never appeared");

	const expression = `
(async () => {
  const omp = window.omp;
  const wanted = ${open};
  let tabs = await omp.tabs.list();
  if (wanted > 0) {
    const cwd = tabs.find(tab => tab.active)?.cwd ?? tabs[0]?.cwd;
    while (tabs.length < wanted) {
      const spawned = await omp.tabs.spawn({ cwd });
      if (!spawned || !spawned.tabId) break;
      tabs = await omp.tabs.list();
    }
    // The app starts its active tab on its own, a few seconds after the renderer mounts.
    // Activating a tab before that happens grabs a different tab as active and boots a
    // second sidecar, so -Tabs 1 would report two. Wait for the active tab itself to reach
    // ready. Use the full budget: a fixed 8s bound falls through on a slow machine and
    // activates a second tab anyway.
    const naturalDeadline = Date.now() + ${waitReady};
    while (Date.now() < naturalDeadline) {
      const active = tabs.find(tab => tab.active);
      if (active?.status === "ready") break;
      await new Promise(resolve => setTimeout(resolve, 200));
      tabs = await omp.tabs.list();
    }
    // Activate only as many extra tabs as N minus the already-ready count. Never touch the
    // active tab: the app is already starting it.
    const missing = wanted - tabs.filter(tab => tab.status === "ready").length;
    if (missing > 0) {
      const candidates = tabs.filter(tab => tab.status !== "ready" && !tab.active);
      for (const tab of candidates.slice(0, missing)) {
        await omp.tabs.setActive(tab.tabId);
        await new Promise(resolve => setTimeout(resolve, 100));
        tabs = await omp.tabs.list();
      }
    }
  }
  const deadline = Date.now() + ${waitReady};
  tabs = await omp.tabs.list();
  while (
    Date.now() < deadline &&
    wanted > 0 &&
    tabs.filter(tab => tab.status === "ready").length < wanted
  ) {
    await new Promise(resolve => setTimeout(resolve, 200));
    tabs = await omp.tabs.list();
  }
  const markDeadline = Date.now() + ${waitReady};
  const missingMarks = ${JSON.stringify(wantedMarks)};
  while (
    Date.now() < markDeadline &&
    !missingMarks.every(name => performance.getEntriesByName(name).length > 0)
  ) {
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  const marks = performance.getEntriesByType("mark").map(entry => ({
    name: entry.name,
    epochMs: Math.round(performance.timeOrigin + entry.startTime),
  }));
  const result = {
    tabs: tabs.map(tab => ({ tabId: tab.tabId, status: tab.status, active: !!tab.active })),
    marks,
    cwd: tabs[0]?.cwd ?? null,
  };
  if (${close}) {
    for (const tab of tabs) {
      if (!tab.active) await omp.tabs.close(tab.tabId);
    }
  }
  return result;
})()
`;

	const response = await send("Runtime.evaluate", {
		expression,
		awaitPromise: true,
		returnByValue: true,
	});
	if (response.__error) throw new Error(`cdp ${response.__error} while driving tabs`);
	const result = response.result ?? {};
	if (result.exceptionDetails) {
		throw new Error(result.exceptionDetails.text ?? JSON.stringify(result.exceptionDetails));
	}
	socket.close();
	return result.result?.value ?? null;
}

let value = null;
let lastError = null;
for (let i = 0; i < 4; i++) {
	try {
		value = await attempt();
		lastError = null;
		break;
	} catch (error) {
		lastError = error;
		await sleep(500);
	}
}
if (lastError) {
	console.error(lastError.stack ?? String(lastError));
	process.exit(1);
}
const text = JSON.stringify(value, null, 1);
const out = args.get("out");
if (out) writeFileSync(out, text);
console.log(text);
// A socket left open by a failed attempt keeps the event loop alive; the shell then waits
// forever for a process that has already printed its result.
process.exit(0);
