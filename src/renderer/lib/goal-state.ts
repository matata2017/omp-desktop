import type { RpcGoalState } from "../../shared/rpc-types";

/** Statuses that mean the goal is gone, not merely not running. */
const TERMINAL_GOAL_STATUSES: Record<string, true> = { dropped: true, complete: true };

export function isTerminalGoalStatus(status: unknown): boolean {
	return typeof status === "string" && TERMINAL_GOAL_STATUSES[status] === true;
}

/** A goal still exists (and must stay visible) while it is active OR paused. */
export function goalExists(data: RpcGoalState | null | undefined): data is RpcGoalState & { objective: string } {
	if (!data || isTerminalGoalStatus(data.status)) return false;
	return typeof data.objective === "string" && data.objective.trim().length > 0;
}

/** Narrow an untyped goal payload to the fields the session store tracks. */
export function goalFields(value: unknown): { objective?: string; status?: string } | null {
	if (!value || typeof value !== "object") return null;
	const objective = "objective" in value && typeof value.objective === "string" ? value.objective : undefined;
	const status = "status" in value && typeof value.status === "string" ? value.status : undefined;
	return { objective, status };
}

/**
 * Normalize a `goal_updated` frame into a session-store patch.
 *
 * A paused goal keeps its goal object but emits `state.enabled = false`;
 * treating that as "no goal" made the dock bar and editor disappear. Only a
 * terminal status (dropped/complete) or a missing goal object clears the store.
 */
export function goalPatchFromEvent(
	goal: unknown,
	state: unknown,
): { goal: { objective?: string } | null; goalState: { status?: string } | null } {
	const goalInfo = goalFields(goal);
	const stateGoal = state && typeof state === "object" && "goal" in state ? goalFields(state.goal) : null;
	const status = stateGoal?.status ?? goalInfo?.status;
	if (!goalInfo || isTerminalGoalStatus(status)) return { goal: null, goalState: null };
	return { goal: { objective: goalInfo.objective }, goalState: { status } };
}
