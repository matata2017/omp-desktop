import { useEffect } from "react";
import type { RpcSkillsResult } from "../../shared/rpc-types";
import { type TabRpc, useTabRpc } from "../lib/tab-rpc";
import { useSessionStore } from "../stores/session";
import { useSkillReviewStore } from "../stores/skill-review";
import { useActiveTabRouteReady } from "./use-active-tab-route";

/** Re-read the learned skills through `rpc`; a failed read keeps the last list. */
export async function refreshLearnedSkills(rpc: Pick<TabRpc, "getSkills">): Promise<void> {
	try {
		const response = await rpc.getSkills();
		if (!response.success) return;
		useSkillReviewStore.getState().setLearned((response.data as RpcSkillsResult | undefined)?.skills ?? []);
	} catch {
		// The sidebar badge is advisory; the next change notice or open retries.
	}
}

/**
 * Keeps the learned-skill review count current for the whole window: once the
 * sidecar is up, and whenever omp writes a learned skill (from any session or
 * process — the main process watches the folder).
 */
export function useLearnedSkillsSync(): void {
	const rpc = useTabRpc();
	const ready = useSessionStore(state => state.status === "ready");
	const routeReady = useActiveTabRouteReady();

	useEffect(() => {
		void useSkillReviewStore.getState().hydrate();
	}, []);

	useEffect(() => {
		if (!ready || !routeReady) return;
		void refreshLearnedSkills(rpc);
		return window.omp.events.onMemoryChanged(change => {
			if (change.skills) void refreshLearnedSkills(rpc);
		});
	}, [ready, routeReady, rpc]);
}
