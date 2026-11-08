/**
 * The close-a-tab rules, shared by the chip's ×, ⌘W and File → Close Tab.
 *
 * A tab whose run is live dies with its close, so it arms the chip's inline
 * confirm first; a worktree-bound tab detours to the cleanup prompt (plan/20)
 * instead of orphaning the checkout; and at the single-tab floor the last tab
 * stays. Tauri has one window: `window.close()` there quits the whole app, so
 * it must never stand in for closing a tab.
 */

import { useAgentPagesStore } from "../stores/agent-pages";
import { useSessionStore } from "../stores/session";
import { type SessionTab, useTabsStore } from "../stores/tabs";
import { useUiStore } from "../stores/ui";
import { tabSignalPresentation } from "./tab-signal";

/** Foreground knowledge the pool's status pushes lag behind: the active tab's
 *  own stream, which only the renderer sees the instant it starts. */
export interface LiveTabRuntime {
	activeTabId: string | null;
	streaming: boolean;
	compacting: boolean;
}

/** True when closing this tab destroys work, so the close needs a confirmation. */
export function tabNeedsCloseConfirm(tab: SessionTab, live: LiveTabRuntime): boolean {
	const activeRuntime = tab.id === live.activeTabId && (live.streaming || live.compacting);
	return tabSignalPresentation(tab, activeRuntime).running || tab.status === "starting";
}

/** What a close request ended up doing. `"page"` = a subagent page was shown,
 *  so ⌘W closed it; `"nothing"` = no tab to close (also the single-tab floor). */
export type TabCloseOutcome = "closed" | "worktree-prompt" | "armed" | "page" | "nothing";

/** Execute a close the user already committed to (chip ✓, or ⌘W on an armed tab). */
export function performTabClose(tab: SessionTab): TabCloseOutcome {
	const ui = useUiStore.getState();
	ui.cancelCloseTab();
	if (tab.worktree) {
		ui.openWorktreeClosePrompt(tab.id);
		return "worktree-prompt";
	}
	void useTabsStore.getState().closeTab(tab.id);
	return "closed";
}

/** ⌘W / File → Close Tab: close the active tab under the shared rules. */
export function closeActiveTab(): TabCloseOutcome {
	const { tabs, activeTabId } = useTabsStore.getState();
	const session = useSessionStore.getState();
	const target = tabs.find(tab => tab.id === activeTabId);
	if (!target) return "nothing";
	// A selected child chip closes like a tab and shows its session (plan/24 §3.2).
	const pages = useAgentPagesStore.getState();
	const page = pages.shown[target.id];
	if (page) {
		pages.close(target.id, page);
		return "page";
	}
	if (tabs.length <= 1) {
		// One window in Tauri: closing it terminates the whole app. Keep the last
		// tab instead; quitting goes through the `app:quit` guard.
		return "nothing";
	}
	const live: LiveTabRuntime = {
		activeTabId,
		streaming: session.isStreaming,
		compacting: session.isCompacting,
	};
	const ui = useUiStore.getState();
	if (tabNeedsCloseConfirm(target, live) && ui.armedCloseTab?.tabId !== target.id) {
		ui.armCloseTab(target.id);
		return "armed";
	}
	return performTabClose(target);
}
