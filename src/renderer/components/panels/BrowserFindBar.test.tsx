import { parseHTML } from "linkedom";
import { act, createRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import { I18nProvider } from "../../lib/i18n";
import { FindBar } from "./BrowserFindBar";

const { document, window, Event, HTMLElement, Element, Node } = parseHTML("<html><body></body></html>");

const globals = globalThis as Record<string, unknown>;
globals.document = document;
globals.window = window;
globals.Event = Event;
globals.HTMLElement = HTMLElement;
globals.Element = Element;
globals.Node = Node;
globals.IS_REACT_ACT_ENVIRONMENT = true;

interface TestElement {
	remove: () => void;
	querySelector: (selector: string) => TestElement | null;
	dispatchEvent: (event: Event) => boolean;
}

let container: TestElement;
let root: Root;
let steps: boolean[];
let closes: number;

async function mount(): Promise<TestElement> {
	steps = [];
	closes = 0;
	container = document.createElement("div") as unknown as TestElement;
	document.body.appendChild(container as never);
	root = createRoot(container as unknown as Element);
	await act(async () => {
		root.render(
			<I18nProvider>
				<FindBar
					inputRef={createRef()}
					text="4242"
					result={{ active: 1, matches: 3 }}
					onSearch={() => {}}
					onStep={backward => steps.push(backward)}
					onClose={() => closes++}
				/>
			</I18nProvider>,
		);
	});
	const input = container.querySelector("input");
	if (!input) throw new Error("no find field");
	return input;
}

/** Key down in the field; returns whether the bar claimed it (default prevented). */
async function press(
	input: TestElement,
	key: string,
	init: { shiftKey?: boolean; isComposing?: boolean; keyCode?: number } = {},
): Promise<boolean> {
	const event = new Event("keydown", { bubbles: true, cancelable: true });
	Object.assign(event, { key, shiftKey: false, isComposing: false, keyCode: 0, ...init });
	await act(async () => {
		input.dispatchEvent(event);
	});
	return event.defaultPrevented;
}

afterEach(async () => {
	await act(async () => {
		root.unmount();
	});
	container.remove();
});

describe("FindBar keys", () => {
	it("steps forward on Enter and backward on Shift+Enter, but not on an IME's confirming Enter", async () => {
		const input = await mount();
		expect(await press(input, "Enter")).toBe(true);
		expect(await press(input, "Enter", { shiftKey: true })).toBe(true);
		// Pinyin and other input methods confirm a word with Enter: it must not jump to a match.
		expect(await press(input, "Enter", { isComposing: true })).toBe(false);
		expect(await press(input, "Enter", { keyCode: 229 })).toBe(false);
		expect(steps).toEqual([false, true]);
	});

	it("claims Escape to close, so the app does not also stop the agent's turn", async () => {
		const input = await mount();
		expect(await press(input, "Escape")).toBe(true);
		expect(closes).toBe(1);
	});
});
