import { describe, expect, it, vi } from "vitest";
import type { RpcCommand, RpcResponse } from "../../shared/rpc-types";
import { ensureRpcGoalContinuation, GOAL_CONTINUATION_PATH, withRpcContinuation } from "./goal-continuation";

function response(data: unknown): RpcResponse {
	return { type: "response", command: "test", success: true, data };
}

describe("withRpcContinuation", () => {
	it("leaves a list that already allows rpc untouched", () => {
		expect(withRpcContinuation(["interactive", "rpc"])).toBeNull();
	});

	it("adds rpc to the sidecar default so goal mode keeps running", () => {
		expect(withRpcContinuation(["interactive"])).toEqual(["interactive", "rpc"]);
		expect(withRpcContinuation(undefined)).toEqual(["rpc"]);
		expect(withRpcContinuation("rpc")).toEqual(["rpc"]);
	});

	it("drops non-string entries instead of writing them back", () => {
		expect(withRpcContinuation(["interactive", 7, null])).toEqual(["interactive", "rpc"]);
	});
});

describe("ensureRpcGoalContinuation", () => {
	it("writes the merged list when the sidecar left rpc out", async () => {
		const command = vi.fn(async (request: RpcCommand) =>
			request.type === "get_settings"
				? response({ values: { [GOAL_CONTINUATION_PATH]: ["interactive"] } })
				: response({ value: ["interactive", "rpc"] }),
		);
		await ensureRpcGoalContinuation(command);
		expect(command).toHaveBeenLastCalledWith({
			type: "set_setting",
			path: GOAL_CONTINUATION_PATH,
			value: ["interactive", "rpc"],
		});
	});

	it("does not write when rpc is already allowed", async () => {
		const command = vi.fn(async () =>
			response({ values: { [GOAL_CONTINUATION_PATH]: ["interactive", "rpc"] } }),
		);
		await ensureRpcGoalContinuation(command);
		expect(command).toHaveBeenCalledTimes(1);
		expect(command).toHaveBeenCalledWith({ type: "get_settings", paths: [GOAL_CONTINUATION_PATH] });
	});

	it("stays silent when the sidecar refuses the read", async () => {
		const command = vi.fn(async () => ({ type: "response", command: "get_settings", success: false }) as RpcResponse);
		await expect(ensureRpcGoalContinuation(command)).resolves.toBeUndefined();
		expect(command).toHaveBeenCalledTimes(1);
	});
});
