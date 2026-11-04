import { afterEach, describe, expect, it } from "vitest";
import type { IpcBrowserPage, IpcBrowserState } from "../../shared/ipc-types";
import { applyBrowserState, useBrowserStore } from "./browser";

function page(pageId: number, openedBy: IpcBrowserPage["openedBy"]): IpcBrowserPage {
	return {
		pageId,
		url: "https://example.com/",
		title: "Example",
		loading: false,
		canGoBack: false,
		canGoForward: false,
		openedBy,
		agentDriving: openedBy === "agent",
	};
}

function state(pages: IpcBrowserPage[]): IpcBrowserState {
	return { tabId: "t1", pages, activePageId: pages.at(-1)?.pageId ?? null, agentLinked: true };
}

afterEach(() => useBrowserStore.setState({ byTab: {}, reveal: null }));

describe("applyBrowserState", () => {
	it("asks to show the browser only when a pushed update carries a page the agent just opened", () => {
		applyBrowserState(state([page(1, "user")]), { reveal: true });
		expect(useBrowserStore.getState().reveal).toBeNull();

		applyBrowserState(state([page(1, "user"), page(2, "agent")]), { reveal: true });
		const first = useBrowserStore.getState().reveal;
		expect(first).toMatchObject({ tabId: "t1" });

		// Navigation inside the agent's existing page is not a new page.
		applyBrowserState(state([page(1, "user"), { ...page(2, "agent"), title: "Next" }]), { reveal: true });
		expect(useBrowserStore.getState().reveal).toBe(first);
	});

	it("leaves the drawer alone for a page the agent opened in the background", () => {
		applyBrowserState(state([page(1, "user")]), { reveal: true });
		applyBrowserState({ ...state([page(1, "user"), page(2, "agent")]), activePageId: 1 }, { reveal: true });
		expect(useBrowserStore.getState().reveal).toBeNull();
	});

	it("never reveals from a state read, such as a reloaded window catching up", () => {
		applyBrowserState(state([page(3, "agent")]), { reveal: false });
		expect(useBrowserStore.getState().reveal).toBeNull();
		expect(useBrowserStore.getState().byTab.t1?.pages).toHaveLength(1);
	});
});
