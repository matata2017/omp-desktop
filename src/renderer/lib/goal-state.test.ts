import { describe, expect, it } from "vitest";
import { goalExists, goalPatchFromEvent } from "./goal-state";

describe("goalPatchFromEvent", () => {
	it("keeps a paused goal visible even though the frame reports enabled=false", () => {
		expect(
			goalPatchFromEvent(
				{ objective: "Ship it", status: "paused" },
				{ enabled: false, goal: { objective: "Ship it", status: "paused" } },
			),
		).toEqual({ goal: { objective: "Ship it" }, goalState: { status: "paused" } });
	});

	it("keeps an active goal", () => {
		expect(
			goalPatchFromEvent(
				{ objective: "Ship it", status: "active" },
				{ enabled: true, goal: { objective: "Ship it", status: "active" } },
			),
		).toEqual({ goal: { objective: "Ship it" }, goalState: { status: "active" } });
	});

	it("clears terminal drop/complete frames", () => {
		for (const status of ["dropped", "complete"]) {
			expect(
				goalPatchFromEvent(
					{ objective: "Ship it", status },
					{ enabled: false, goal: { objective: "Ship it", status } },
				),
			).toEqual({ goal: null, goalState: null });
		}
	});

	it("clears when no goal object is present", () => {
		expect(goalPatchFromEvent(null, { enabled: false, goal: null })).toEqual({ goal: null, goalState: null });
	});
});

describe("goalExists", () => {
	it("keeps paused snapshots returned by get_goal", () => {
		expect(goalExists({ enabled: false, status: "paused", objective: "Ship it" })).toBe(true);
		expect(goalExists({ enabled: true, status: "active", objective: "Ship it" })).toBe(true);
	});

	it("rejects no-goal and terminal snapshots", () => {
		expect(goalExists({ enabled: false })).toBe(false);
		expect(goalExists({ enabled: false, status: "none" })).toBe(false);
		expect(goalExists({ enabled: false, status: "dropped", objective: "Ship it" })).toBe(false);
		expect(goalExists({ enabled: false, status: "complete", objective: "Ship it" })).toBe(false);
		expect(goalExists(undefined)).toBe(false);
	});
});
