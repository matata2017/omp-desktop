// Probe whether one sidecar process can serve two tabs' sessions at once.
// Sends a prompt on session A, switches to session B mid-stream, and records what happens to
// the switch and to A's remaining events. Usage: node scripts/perf/probe-shared.mjs
// Requires two existing session files; pass --a and --b, or let it pick the two newest.
import { spawn } from "node:child_process";
import { readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";

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

const sessionsDir =
	args.get("dir") ?? join(process.env.USERPROFILE ?? "", ".omp", "agent", "sessions", "--D--rust-omp--");
const files = readdirSync(sessionsDir)
	.filter(name => name.endsWith(".jsonl"))
	.map(name => join(sessionsDir, name))
	.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
const sessionA = args.get("a") ?? files[1];
const sessionB = args.get("b") ?? files[0];
if (!sessionA || !sessionB) {
	console.error("need two session files");
	process.exit(1);
}

const sidecar =
	args.get("sidecar") ?? join(process.cwd(), "resources", "omp.exe");
const child = spawn(sidecar, ["--mode", "rpc-ui", "--no-auto-resume"], {
	cwd: process.cwd(),
	stdio: ["pipe", "pipe", "pipe"],
	// Never let a probe write into the user's live data dir.
	env: {
		...process.env,
		OMP_DATA_DIR: args.get("data") ?? join(process.cwd(), ".perf", "probe-data"),
	},
});
child.stderr.on("data", () => {});

const events = [];
let readyResolve;
const ready = new Promise(resolve => {
	readyResolve = resolve;
});
const lines = createInterface({ input: child.stdout });
lines.on("line", line => {
	let frame;
	try {
		frame = JSON.parse(line);
	} catch {
		return;
	}
	events.push({ at: Date.now(), frame });
	if (frame.type === "ready") readyResolve(frame);
});

function send(command) {
	child.stdin.write(`${JSON.stringify(command)}\n`);
}
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const waitFor = async (predicate, timeoutMs) => {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		const match = events.find(predicate);
		if (match) return match;
		await sleep(50);
	}
	return null;
};

try {
	await Promise.race([ready, sleep(15000)]);
	send({ id: "a-switch", type: "switch_session", sessionPath: sessionA });
	const switchA = await waitFor(event => event.frame?.id === "a-switch", 15000);
	send({ id: "a-prompt", type: "prompt", message: "Count from 1 to 30, one number per line." });
	const firstUpdate = await waitFor(
		event => event.frame?.type === "message_update" || event.frame?.type === "agent_start",
		20000,
	);
	const beforeSwitch = events.length;
	send({ id: "b-switch", type: "switch_session", sessionPath: sessionB });
	const switchB = await waitFor(event => event.frame?.id === "b-switch", 15000);
	await sleep(6000);
	const afterSwitch = events.slice(beforeSwitch);
	const typesAfterSwitch = {};
	for (const event of afterSwitch) {
		const type = event.frame?.type ?? "unknown";
		typesAfterSwitch[type] = (typesAfterSwitch[type] ?? 0) + 1;
	}
	const agentEndedAfterSwitch = afterSwitch.some(event => event.frame?.type === "agent_end");
	// Switch back while the turn may still be live: a shared process would have to resume
	// routing A's stream, which is the capability the sharing idea depends on.
	const beforeReturn = events.length;
	send({ id: "a-return", type: "switch_session", sessionPath: sessionA });
	const returnSwitch = await waitFor(event => event.frame?.id === "a-return", 15000);
	await sleep(3000);
	const afterReturn = events.slice(beforeReturn);
	send({ id: "state", type: "get_state" });
	const state = await waitFor(event => event.frame?.id === "state", 15000);
	send({ id: "abort", type: "abort" });
	await sleep(300);
	console.log(
		JSON.stringify(
			{
				sessions: { a: sessionA, b: sessionB },
				ready: events.some(event => event.frame?.type === "ready"),
				sessionASwitch: switchA?.frame ?? null,
				firstStreamEvent: firstUpdate?.frame?.type ?? null,
				sessionBSwitch: switchB?.frame ?? null,
				eventsAfterSwitch: afterSwitch.length,
				typesAfterSwitch,
				agentEndedAfterSwitch,
				returnSwitch: returnSwitch?.frame ?? null,
				messageUpdatesAfterReturn: afterReturn.filter(
					event => event.frame?.type === "message_update",
				).length,
				stateSession:
					state?.frame?.data?.sessionPath ?? state?.frame?.data?.session?.path ?? null,
			},
			null,
			1,
		),
	);
} finally {
	child.kill();
	process.exit(0);
}
