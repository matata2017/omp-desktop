/**
 * Skill review when omp rewrites a skill the person already reviewed, while its
 * card is open: the card shows what changed — never re-reviews the words read
 * before the rewrite — and the review bookmark is written a bounded number of
 * times (a stale read once re-marked it in a render loop that froze the app).
 */
import { parseHTML } from "linkedom";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import type { RpcSkillDetail, RpcSkillInfo } from "../../../shared/rpc-types";
import { I18nProvider } from "../../lib/i18n";
import { useSessionStore } from "../../stores/session";
import { useSkillReviewStore } from "../../stores/skill-review";
import { SkillsView } from "./SkillsView";

const { document, window, Event, HTMLElement, Node } = parseHTML("<html><body></body></html>");
const globals = globalThis as Record<string, unknown>;
globals.document = document;
globals.window = window;
globals.Event = Event;
globals.HTMLElement = HTMLElement;
globals.Node = Node;
globals.IS_REACT_ACT_ENVIRONMENT = true;
globals.requestAnimationFrame = (callback: () => void) => setTimeout(callback, 0);

const ok = (data?: unknown) => ({ type: "response" as const, command: "x", success: true as const, data });

function info(modifiedAt: number): RpcSkillInfo {
	return {
		name: "release-build",
		description: "Build and sign a release",
		source: "omp-managed:user",
		enabled: true,
		location: "/skills/release-build/SKILL.md",
		provider: "omp-managed",
		providerName: "Managed",
		level: "user",
		managed: true,
		hidden: false,
		modifiedAt,
	};
}

const V1 = "# Release\n\n1. build\n2. sign\n";
const V2 = "# Release\n\n1. build\n2. test\n3. sign\n";

let container: HTMLElement;
let root: Root;
let current: { modifiedAt: number; body: string };
let prefsSet: Mock;

async function flush(rounds = 3): Promise<void> {
	for (let i = 0; i < rounds; i++) {
		await act(async () => {
			const { promise, resolve } = Promise.withResolvers<void>();
			setTimeout(resolve, 0);
			await promise;
		});
	}
}

function reactProps(element: Element | null): Record<string, ((event: object) => unknown) | undefined> {
	const record = element as unknown as Record<string, unknown>;
	const key = Object.getOwnPropertyNames(record).find(name => name.startsWith("__reactProps$"));
	return (key ? record[key] : {}) as Record<string, ((event: object) => unknown) | undefined>;
}

beforeEach(async () => {
	current = { modifiedAt: 100, body: V1 };
	prefsSet = vi.fn(async () => {});
	(window as unknown as { omp: unknown }).omp = {
		prefs: { get: vi.fn(async () => undefined), set: prefsSet },
		rpc: {
			getSkills: vi.fn(async () => ok({ skills: [info(current.modifiedAt)] })),
			getSkillDetail: vi.fn(async () =>
				ok({ ...info(current.modifiedAt), body: current.body } satisfies RpcSkillDetail),
			),
			getSettings: vi.fn(async () => ok({ values: { "autolearn.enabled": true } })),
		},
	};
	useSessionStore.setState({ status: "ready" });
	useSkillReviewStore.setState({
		hydrated: true,
		learned: [info(100)],
		reviewed: {
			"release-build": { modifiedAt: 100, description: "Build and sign a release", body: V1, reviewedAt: 1 },
		},
	});
	container = document.createElement("div") as unknown as HTMLElement;
	document.body.appendChild(container);
	root = createRoot(container);
	await act(async () => {
		root.render(
			<I18nProvider>
				<SkillsView open />
			</I18nProvider>,
		);
	});
	await flush();
});

afterEach(async () => {
	await act(async () => root.unmount());
	container.remove();
	vi.restoreAllMocks();
});

async function rewrite(modifiedAt: number, body: string): Promise<void> {
	current = { modifiedAt, body };
	await act(async () => useSkillReviewStore.getState().setLearned([info(modifiedAt)]));
	await flush(6);
}

describe("SkillsView review of a rewritten skill", () => {
	it("shows what changed instead of re-reviewing the earlier read", async () => {
		const card = container.querySelector("button[aria-expanded]");
		await act(async () => reactProps(card).onClick?.({}));
		await flush();
		expect(container.textContent).toContain("Remove");

		await rewrite(200, V2);

		expect(container.textContent).toContain("Updated");
		expect(container.textContent).toContain("What changed since you reviewed it");
		expect(useSkillReviewStore.getState().reviewed["release-build"]?.modifiedAt).toBe(100);
		expect(prefsSet).not.toHaveBeenCalled();
	});

	it("marks a rewrite with the same words reviewed once", async () => {
		const card = container.querySelector("button[aria-expanded]");
		await act(async () => reactProps(card).onClick?.({}));
		await flush();

		await rewrite(300, V1);

		expect(useSkillReviewStore.getState().reviewed["release-build"]?.modifiedAt).toBe(300);
		expect(container.textContent).not.toContain("Updated");
		expect(prefsSet).toHaveBeenCalledTimes(1);
	});
});
