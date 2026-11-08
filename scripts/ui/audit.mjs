// UI audit: copy patterns in the locale files plus an axe pass and a screenshot of the
// running window. Usage: node scripts/ui/audit.mjs [--port 9222] [--out docs/ui-audit.md]
// Writes the report and docs/ui-audit/screen.png. Axe is skipped when no debug port answers.
import { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";

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
const outPath = args.get("out") ?? "docs/ui-audit.md";
const shotDir = args.get("shots") ?? "docs/ui-audit";

const LOCALES = ["src/renderer/locales/en.ts", "src/renderer/locales/zh.ts", "src/renderer/locales/ru.ts"];
const ROOT = "src/renderer";

const BANNED = [
	"crucial",
	"delve",
	"seamless",
	"robust",
	"effortless",
	"elevate",
	"unlock",
	"empower",
	"leverage",
	"showcase",
	"foster",
	"tapestry",
	"testament",
	"underscore",
	"pivotal",
	"intricate",
];
const PATTERNS = [
	{ name: "em dash", test: text => text.includes("\u2014") },
	{ name: "curly quote", test: text => /[\u201c\u201d\u2018\u2019]/.test(text) },
	{
		name: "AI vocabulary",
		test: text => {
			const lower = text.toLowerCase();
			return BANNED.find(word => lower.includes(word)) ?? null;
		},
	},
];

/** Every string rendered through t("key") in the renderer, counted. */
function usageCounts() {
	const counts = new Map();
	const visit = dir => {
		for (const entry of readdirSync(dir)) {
			const path = join(dir, entry);
			const stat = statSync(path);
			if (stat.isDirectory()) {
				visit(path);
				continue;
			}
			if (!/\.(ts|tsx)$/.test(entry) || entry.endsWith(".test.ts") || entry.endsWith(".test.tsx")) continue;
			const text = readFileSync(path, "utf8");
			for (const match of text.matchAll(/\bt\(\s*["']([^"'`]+)["']/g)) {
				counts.set(match[1], (counts.get(match[1]) ?? 0) + 1);
			}
		}
	};
	visit(ROOT);
	return counts;
}

function parseLocale(path) {
	const out = [];
	const text = readFileSync(path, "utf8");
	for (const line of text.split(/\r?\n/)) {
		const match = line.match(/^\s*(?:"([^"]+)"|'([^']+)'|([A-Za-z0-9_.-]+))\s*:\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')\s*,?\s*$/);
		if (!match) continue;
		const key = match[1] ?? match[2] ?? match[3];
		const value = match[4] ?? match[5] ?? "";
		out.push({ key, value: value.replace(/\\"/g, '"').replace(/\\n/g, "\n") });
	}
	return out;
}

const counts = usageCounts();
const findings = [];
for (const locale of LOCALES) {
	for (const entry of parseLocale(locale)) {
		const hits = [];
		for (const pattern of PATTERNS) {
			const hit = pattern.test(entry.value);
			if (hit) hits.push(pattern.name === "AI vocabulary" ? `AI vocabulary: ${hit}` : pattern.name);
		}
		if (hits.length === 0) continue;
		findings.push({
			locale: locale.split("/").pop(),
			key: entry.key,
			value: entry.value.replace(/\s+/g, " ").slice(0, 140),
			hits,
			uses: counts.get(entry.key) ?? 0,
		});
	}
}
findings.sort((a, b) => b.uses - a.uses || a.key.localeCompare(b.key));

async function cdpAudit() {
	const targets = await (await fetch(`http://127.0.0.1:${port}/json`, { signal: AbortSignal.timeout(3000) })).json();
	const target =
		targets.find(item => item.type === "page" && item.url.includes("localhost:1420")) ??
		targets.find(item => item.type === "page" && !item.url.startsWith("http")) ??
		targets.find(item => item.type === "page");
	if (!target) throw new Error("no page target");
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
			pending.set(id, resolve);
			socket.send(JSON.stringify({ id, method, params }));
		});

	const axeSource = readFileSync("node_modules/axe-core/axe.min.js", "utf8");
	await send("Runtime.evaluate", { expression: axeSource, returnByValue: false });
	const axe = await send("Runtime.evaluate", {
		expression:
			"axe.run(document, { resultTypes: ['violations'] }).then(r => r.violations.map(v => ({ id: v.id, impact: v.impact, help: v.help, nodes: v.nodes.length })))",
		awaitPromise: true,
		returnByValue: true,
	});
	const violations = axe.result?.result?.value ?? [];
	const shot = await send("Page.captureScreenshot", { format: "png" });
	const data = shot.result?.data;
	socket.close();
	return { violations, screenshot: data };
}

let axe = { violations: null, screenshot: null, error: null };
try {
	axe = await cdpAudit();
} catch (error) {
	axe.error = error.message;
}

mkdirSync(shotDir, { recursive: true });
if (axe.screenshot) {
	writeFileSync(join(shotDir, "screen.png"), Buffer.from(axe.screenshot, "base64"));
}

const lines = [];
lines.push("# UI audit");
lines.push("");
lines.push(`Generated ${new Date().toISOString()} from the locale files and a running window.`);
lines.push("");
lines.push("## Copy findings");
lines.push("");
lines.push("Sorted by how often the key is used in `src/renderer`. A hit list, not a rewrite order.");
lines.push("");
lines.push("| uses | locale | key | hits | text |");
lines.push("|---:|---|---|---|---|");
if (findings.length === 0) {
	lines.push("| 0 | - | - | none | - |");
}
for (const finding of findings.slice(0, 100)) {
	const text = finding.value.replace(/\|/g, "\\|");
	lines.push(`| ${finding.uses} | ${finding.locale} | \`${finding.key}\` | ${finding.hits.join(", ")} | ${text} |`);
}
lines.push("");
lines.push(`Total findings: ${findings.length}. Zero-use keys are likely stale; check before editing.`);
lines.push("");
lines.push("## Accessibility");
lines.push("");
if (axe.violations) {
	if (axe.violations.length === 0) {
		lines.push("axe-core reported no violations.");
	} else {
		lines.push("| impact | id | nodes | help |");
		lines.push("|---|---|---:|---|");
		for (const violation of axe.violations) {
			lines.push(`| ${violation.impact ?? ""} | ${violation.id} | ${violation.nodes} | ${violation.help} |`);
		}
	}
} else {
	lines.push(`axe-core did not run: ${axe.error}. Start the app with the debug port and rerun.`);
}
lines.push("");
lines.push(`Screenshot: \`${relative(process.cwd(), join(shotDir, "screen.png")).replace(/\\/g, "/")}\`.`);
lines.push("");

mkdirSync(join(outPath, ".."), { recursive: true });
writeFileSync(outPath, lines.join("\n"));
console.log(`wrote ${outPath}`);
console.log(`copy_findings=${findings.length} axe_violations=${axe.violations ? axe.violations.length : "skipped"}`);
