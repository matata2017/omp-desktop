/**
 * Memory window view-model: the status line reports the most urgent state,
 * Sources groups put work in progress and problems first, and the project
 * picker names folders the way the sidebar does.
 */
import { describe, expect, it } from "vitest";
import type { SessionInfo } from "../../../shared/ipc-types";
import type { RpcMemoryOverview, RpcMemorySource } from "../../../shared/rpc-types";
import { groupSources, memoryProjects, memoryStatus } from "./memory-model";

function source(threadId: string, state: RpcMemorySource["state"], updatedAt = 0): RpcMemorySource {
	return { threadId, state, updatedAt };
}

function overview(patch: Partial<RpcMemoryOverview>): RpcMemoryOverview {
	return {
		cwd: "/p",
		root: "/m",
		backend: "local",
		enabled: true,
		notes: [],
		lessons: [],
		summary: "",
		full: "",
		playbooks: [],
		injection: {
			tokens: { notes: 0, summary: 0, learned: 0, budget: 5000 },
			notesIncluded: 0,
			lessonsIncluded: 0,
			summaryTruncated: false,
		},
		sources: [],
		consolidation: { pending: false },
		limits: { idleHours: 12, maxAgeDays: 30, perUpdate: 64 },
		models: {},
		...patch,
	};
}

describe("memoryStatus", () => {
	it("reports progress over a past failure, a failure over stale knowledge, and counts what is ready", () => {
		const failed = { pending: false, error: "model timed out", updatedAt: 1 };
		const remembered = [source("a", "remembered"), source("b", "queued"), source("c", "waiting")];

		expect(
			memoryStatus(
				overview({
					consolidation: failed,
					activity: { phase: "reading", done: 1, total: 3, elsewhere: false },
				}),
			),
		).toEqual({ kind: "reading", done: 1, total: 3, elsewhere: false });
		expect(memoryStatus(overview({ consolidation: failed, sources: remembered }))).toEqual({
			kind: "failed",
			error: "model timed out",
		});
		expect(
			memoryStatus(overview({ sources: remembered, summary: "x", consolidation: { pending: false, updatedAt: 9 } })),
		).toEqual({ kind: "current", sessions: 1, updatedAt: 9, ready: 1 });
		expect(memoryStatus(overview({ sources: [source("b", "queued")] }))).toEqual({ kind: "empty", ready: 1 });
		expect(memoryStatus(overview({ enabled: false, sources: remembered }))).toEqual({ kind: "off" });
	});
});

describe("groupSources", () => {
	it("orders pending work as reading, failed, ready, waiting and only counts too-old sessions", () => {
		const groups = groupSources([
			source("wait", "waiting", 50),
			source("ready-old", "queued", 1),
			source("ready-new", "queued", 2),
			source("fail", "failed"),
			source("read", "reading"),
			source("old1", "old"),
			source("old2", "old"),
			source("gone", "forgotten"),
		]);
		expect(groups.pending.map(item => item.threadId)).toEqual(["read", "fail", "ready-new", "ready-old", "wait"]);
		expect(groups.old).toBe(2);
		expect(groups.forgotten.map(item => item.threadId)).toEqual(["gone"]);
	});
});

describe("memoryProjects", () => {
	const session = (cwd: string, modified: string, kind?: "chat"): SessionInfo => ({
		path: `${cwd}/${modified}.jsonl`,
		id: modified,
		title: null,
		cwd,
		created: modified,
		modified,
		messageCount: 1,
		size: 1,
		status: "complete",
		firstMessage: "",
		...(kind ? { kind } : {}),
	});

	it("lists agent projects by recency with sidebar names, and always offers the current one", () => {
		const projects = memoryProjects(
			[
				session("/code/alpha", "2026-01-01T00:00:00Z"),
				session("/code/beta", "2026-03-01T00:00:00Z"),
				session("/code/alpha", "2026-04-01T00:00:00Z"),
				session("/chats", "2026-05-01T00:00:00Z", "chat"),
				session("/home/me/work", "2026-02-01T00:00:00Z"),
			],
			{ current: "/code/new", aliases: { "/code/beta": "Beta app" }, workCwd: "/home/me/work", workLabel: "Work" },
		);
		expect(projects.map(project => [project.cwd, project.label])).toEqual([
			["/code/new", "new"],
			["/code/alpha", "alpha"],
			["/code/beta", "Beta app"],
			["/home/me/work", "Work"],
		]);
	});
});
