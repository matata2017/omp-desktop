/**
 * Theme picker click path: selecting a card must persist the named selection
 * and the legacy scheme (the two fire-and-forget `prefs.set` calls) and reopen
 * on that card.
 */
import { parseHTML } from "linkedom";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";
import { I18nProvider } from "../../lib/i18n";
import { THEMES } from "../../lib/themes";
import { useCustomThemesStore } from "../../stores/custom-themes";
import { useUiStore } from "../../stores/ui";
import { ThemePickerDialog } from "./ThemePickerDialog";

const { document, window, Event, HTMLElement, Node } = parseHTML("<html><body></body></html>");
Object.assign(globalThis as Record<string, unknown>, {
	document,
	window,
	Event,
	HTMLElement,
	Node,
	IS_REACT_ACT_ENVIRONMENT: true,
	requestAnimationFrame: (callback: () => void) => setTimeout(callback, 0),
});

const htmlPrototype = HTMLElement.prototype as unknown as { scrollIntoView?: () => void };
htmlPrototype.scrollIntoView = () => { };

const ompWindow = window as unknown as { omp?: unknown };

let prefsStore: Map<string, unknown>;
let prefsSet: Mock<(key: string, value: unknown) => Promise<void>>;
let prefsGet: Mock<(key?: string) => Promise<unknown>>;
let root: Root | null = null;
let host: HTMLElement | null = null;

function installOmp(): void {
	prefsStore = new Map();
	prefsSet = vi.fn(async (key: string, value: unknown) => {
		prefsStore.set(key, value);
	});
	prefsGet = vi.fn(async (key?: string) =>
		key === undefined ? Object.fromEntries(prefsStore) : (prefsStore.get(key) ?? null),
	);
	const omp = {
		prefs: { get: prefsGet, set: prefsSet, onLanguageChanged: vi.fn(() => () => { }) },
		themes: { list: vi.fn(async () => ({ dir: null, files: [] })), onChanged: vi.fn(() => () => { }) },
	};
	ompWindow.omp = omp;
}

async function flush(): Promise<void> {
	const { promise, resolve } = Promise.withResolvers<void>();
	setTimeout(resolve, 0);
	await act(async () => promise);
}

async function mount(): Promise<void> {
	useUiStore.setState({ themePickerOpen: true });
	host = document.createElement("div") as unknown as HTMLElement;
	document.body.appendChild(host as never);
	root = createRoot(host as unknown as Element);
	await act(async () => {
		root?.render(
			<I18nProvider>
				<ThemePickerDialog />
			</I18nProvider>,
		);
	});
	await flush();
	await flush();
}

function themeCard(label: string): HTMLElement | undefined {
	return [...document.body.querySelectorAll("button[aria-pressed]")].find(button =>
		(button.textContent ?? "").includes(label),
	) as unknown as HTMLElement | undefined;
}

beforeEach(() => {
	installOmp();
	useUiStore.setState({ themePickerOpen: false });
	useCustomThemesStore.setState({ dir: null, entries: [] });
});

afterEach(async () => {
	await act(async () => root?.unmount());
	root = null;
	host?.remove();
	host = null;
	document.body.innerHTML = "";
	useUiStore.setState({ themePickerOpen: false });
	useCustomThemesStore.setState({ dir: null, entries: [] });
	delete ompWindow.omp;
});

describe("ThemePickerDialog persistence", () => {
	it("persists both the named theme and its scheme when a card is clicked", async () => {
		await mount();
		const card = themeCard("Slate");
		if (!card) throw new Error("Slate theme card missing");

		await act(async () => card.click());
		await flush();

		expect(prefsSet).toHaveBeenCalledWith("themeName", "titanium");
		expect(prefsSet).toHaveBeenCalledWith("theme", THEMES.titanium.scheme);
		expect(prefsStore.get("themeName")).toBe("titanium");
	});

	it("reopens on the persisted theme card", async () => {
		prefsStore.set("themeName", "titanium");
		await mount();

		const selected = [...document.body.querySelectorAll("button[aria-pressed='true']")];
		expect(selected.some(button => (button.textContent ?? "").includes("Slate"))).toBe(true);
	});
});
