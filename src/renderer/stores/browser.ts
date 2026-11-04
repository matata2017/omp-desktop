/**
 * Every tab's built-in browser as main reports it (pages, navigation, the
 * agent's link). Main owns the pages; this is a mirror for the Browser panel.
 */
import { create } from "zustand";
import type { IpcBrowserState } from "../../shared/ipc-types";

interface BrowserStore {
	byTab: Record<string, IpcBrowserState>;
	/** The last tab whose agent opened a page: the drawer shows its browser. */
	reveal: { tabId: string; nonce: number } | null;
}

export const useBrowserStore = create<BrowserStore>()(() => ({ byTab: {}, reveal: null }));

/**
 * Store a tab's browser state. A pushed update (`reveal`) that carries a page
 * the agent just opened in front asks for the browser to be shown; one it
 * opened in the background does not, and neither does a state read on mount,
 * or reloading the window would yank the drawer around.
 */
export function applyBrowserState(state: IpcBrowserState, options: { reveal: boolean }): void {
	const previous = useBrowserStore.getState().byTab[state.tabId];
	const known = new Set(previous?.pages.map(page => page.pageId));
	const agentOpened =
		options.reveal &&
		state.pages.some(
			page => page.openedBy === "agent" && page.pageId === state.activePageId && !known.has(page.pageId),
		);
	useBrowserStore.setState(store => ({
		byTab: { ...store.byTab, [state.tabId]: state },
		...(agentOpened ? { reveal: { tabId: state.tabId, nonce: Date.now() } } : {}),
	}));
}

export function useTabBrowser(tabId: string | null): IpcBrowserState | null {
	return useBrowserStore(store => (tabId ? (store.byTab[tabId] ?? null) : null));
}

export function forgetTabBrowser(tabId: string): void {
	useBrowserStore.setState(store => {
		const { [tabId]: _removed, ...byTab } = store.byTab;
		return { byTab };
	});
}
