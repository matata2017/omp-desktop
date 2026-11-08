// Record one real streamed answer with the CDP profiler and report where the renderer spends
// its main-thread time. Usage: node scripts/perf/profile-stream.mjs [--port 9222] [--out file]
// Requires a running app with the debug port, a configured model, and an active agent tab.
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
const out = args.get("out") ?? ".perf/stream-profile.json";
const prompt = args.get("prompt") ?? "Reply with exactly: ok";
const timeoutMs = Number(args.get("timeout") ?? 90000);

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const target =
	targets.find(item => item.type === "page" && item.url.includes("localhost:1420")) ??
	targets.find(item => item.type === "page" && item.url.includes("tauri.localhost")) ??
	targets.find(item => item.type === "page" && !item.url.startsWith("http")) ??
	targets.find(item => item.type === "page");
if (!target) {
	console.error("no page target");
	process.exit(1);
}

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
	socket.addEventListener("open", resolve);
	socket.addEventListener("error", reject);
});
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
const send = (method, params) =>
	new Promise(resolve => {
		const id = nextId++;
		const timer = setTimeout(() => {
			if (pending.delete(id)) resolve({ __error: "timeout" });
		}, 15000);
		pending.set(id, message => {
			clearTimeout(timer);
			resolve(message);
		});
		socket.send(JSON.stringify({ id, method, params }));
	});
const evaluate = async expression => {
	const response = await send("Runtime.evaluate", {
		expression,
		awaitPromise: true,
		returnByValue: true,
	});
	if (response.__error) throw new Error(`cdp ${response.__error}`);
	if (response.result?.exceptionDetails) {
		throw new Error(response.result.exceptionDetails.text ?? "evaluation failed");
	}
	return response.result?.result?.value;
};

await send("Profiler.enable", {});
await send("Performance.enable", {});
const metricsBefore = await send("Performance.getMetrics", {});
await evaluate(`
  window.__perfEvents = [];
  window.omp.events.onBatch(batch => { window.__perfEvents.push(...batch); });
  "subscribed"
`);
await send("Profiler.start", {});
const promptStarted = Date.now();
const promptResult = await evaluate(`window.omp.rpc.prompt(${JSON.stringify(prompt)})`);
let ended = false;
let lastEvent = null;
while (Date.now() - promptStarted < timeoutMs) {
	await sleep(200);
	const events = await evaluate("window.__perfEvents.splice(0)");
	if (Array.isArray(events)) {
		for (const event of events) {
			lastEvent = event?.type ?? lastEvent;
			if (event?.type === "agent_end") {
				ended = true;
				break;
			}
		}
	}
	if (ended) break;
}
if (!ended) {
	await evaluate(`window.omp.rpc.abort()`).catch(() => {});
}
const profile = await send("Profiler.stop", {});
const metricsAfter = await send("Performance.getMetrics", {});
socket.close();

const profileData = profile.result?.profile ?? {};
const byNode = new Map();
const nodes = profileData.nodes ?? [];
for (const node of nodes) {
	byNode.set(node.id, {
		function: node.callFrame.functionName || "(anonymous)",
		url: node.callFrame.url || "",
		line: node.callFrame.lineNumber,
		self: 0,
	});
}
const deltas = profileData.timeDeltas ?? [];
const samples = profileData.samples ?? [];
for (let i = 0; i < samples.length; i++) {
	const node = byNode.get(samples[i]);
	if (node) node.self += (deltas[i] ?? 0) / 1000;
}
const top = [...byNode.values()]
	.filter(node => node.self > 0)
	.sort((a, b) => b.self - a.self)
	.slice(0, 20);

const metric = (message, name) => message.result?.metrics?.find(item => item.name === name)?.value ?? null;
const delta = name => {
	const before = metric(metricsBefore, name);
	const after = metric(metricsAfter, name);
	return before === null || after === null ? null : Math.round((after - before) * 1000) / 1000;
};
const result = {
	prompt,
	ended,
	timeoutMs,
	durationMs: Date.now() - promptStarted,
	lastEvent,
	promptResult,
	metrics: {
		TaskDurationMs: delta("TaskDuration"),
		ScriptDurationMs: delta("ScriptDuration"),
		LayoutCount: delta("LayoutCount"),
		RecalcStyleCount: delta("RecalcStyleCount"),
		JSHeapUsedBytes: delta("JSHeapUsedSize"),
	},
	top: top.map(node => ({
		function: node.function,
		url: node.url.split("/").pop() ?? node.url,
		line: node.line,
		selfMs: Math.round(node.self * 10) / 10,
	})),
};
const text = JSON.stringify(result, null, 1);
writeFileSync(out, text);
console.log(text);
process.exit(0);
