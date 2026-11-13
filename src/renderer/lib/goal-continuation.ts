/**
 * Goal mode continues by itself between turns only for the run modes listed in
 * `goal.continuationModes`. The sidecar default is `["interactive"]`, so a host
 * driving it over `rpc-ui` gets one turn and then silence (measured on 18.19.0:
 * 1 turn in 18 s without "rpc", 4 turns with it). Romp therefore adds "rpc" to
 * that list when a session hydrates, keeping whatever the user already chose.
 */
import type { RpcCommand, RpcResponse } from "../../shared/rpc-types";

export const GOAL_CONTINUATION_PATH = "goal.continuationModes";

/** The list to write, or null when "rpc" is already allowed (no write needed). */
export function withRpcContinuation(current: unknown): string[] | null {
	const list = Array.isArray(current)
		? current.filter((mode): mode is string => typeof mode === "string" && mode !== "")
		: [];
	if (list.includes("rpc")) return null;
	return [...list, "rpc"];
}

export type CommandChannel = (request: RpcCommand) => Promise<RpcResponse>;

export async function ensureRpcGoalContinuation(
	command: CommandChannel,
	path: string = GOAL_CONTINUATION_PATH,
): Promise<void> {
	try {
		const read = await command({ type: "get_settings", paths: [path] });
		if (!read.success) return;
		const values = (read.data as { values?: Record<string, unknown> } | undefined)?.values;
		const next = withRpcContinuation(values?.[path]);
		if (next) await command({ type: "set_setting", path, value: next });
	} catch {
		// A sidecar without the setting keeps its own behaviour; nothing to repair.
	}
}
