/**
 * Inline note/lesson editing: Enter saves (never mid-IME-composition), an edit
 * that lost a race keeps the editor open with the reason, and deleting takes a
 * second click.
 */
import { parseHTML } from "linkedom";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RpcMemoryEditOutcome, RpcMemoryListEntry } from "../../../shared/rpc-types";
import { I18nProvider } from "../../lib/i18n";
import { MemoryEntryRow, useArmed } from "./MemoryEntries";

const { document, window, Event, HTMLElement, Node } = parseHTML("<html><body></body></html>");
const globals = globalThis as Record<string, unknown>;
globals.document = document;
globals.window = window;
globals.Event = Event;
globals.HTMLElement = HTMLElement;
globals.Node = Node;
globals.IS_REACT_ACT_ENVIRONMENT = true;
globals.requestAnimationFrame = (callback: () => void) => setTimeout(callback, 0);
const elementPrototype = HTMLElement.prototype as unknown as Record<string, unknown>;
if (typeof elementPrototype.focus !== "function") elementPrototype.focus = () => {};
if (typeof elementPrototype.setSelectionRange !== "function") elementPrototype.setSelectionRange = () => {};

type Handlers = Record<string, ((event: object) => unknown) | undefined>;

let container: HTMLElement;
let root: Root;

async function flush(): Promise<void> {
	await act(async () => {
		const { promise, resolve } = Promise.withResolvers<void>();
		setTimeout(resolve, 0);
		await promise;
	});
}

/** React's handlers on a rendered element (linkedom cannot drive React's synthetic events). */
function handlers(element: Element | null): Handlers {
	const record = element as unknown as Record<string, unknown>;
	const key = Object.getOwnPropertyNames(record).find(name => name.startsWith("__reactProps$"));
	return (key ? record[key] : {}) as Handlers;
}

async function call(element: Element | null, name: string, event: object = {}): Promise<void> {
	await act(async () => {
		await handlers(element)[name]?.({ preventDefault() {}, stopPropagation() {}, ...event });
	});
	await flush();
}

function button(label: string): Element | null {
	return (
		[...container.querySelectorAll("button")].find(
			item => item.getAttribute("aria-label") === label || item.textContent === label,
		) ?? null
	);
}

const entry: RpcMemoryListEntry = { line: "- Use bun, not npm", text: "Use bun, not npm" };

function Row({
	onSave,
	onDelete,
}: {
	onSave: (text: string) => Promise<RpcMemoryEditOutcome | null>;
	onDelete: () => Promise<RpcMemoryEditOutcome | null>;
}) {
	const { armed, arm } = useArmed();
	return (
		<ul>
			<MemoryEntryRow
				armed={armed === "row"}
				entry={entry}
				kind="note"
				onArm={() => arm("row")}
				onDelete={onDelete}
				onSave={onSave}
			/>
		</ul>
	);
}

async function mount(props: Parameters<typeof Row>[0]): Promise<void> {
	container = document.createElement("div") as unknown as HTMLElement;
	document.body.appendChild(container);
	root = createRoot(container);
	await act(async () => {
		root.render(
			<I18nProvider>
				<Row {...props} />
			</I18nProvider>,
		);
	});
	await flush();
}

afterEach(async () => {
	await act(async () => root.unmount());
	container.remove();
	vi.restoreAllMocks();
});

const enter = (composing = false) => ({ key: "Enter", shiftKey: false, nativeEvent: { isComposing: composing } });

describe("MemoryEntryRow", () => {
	it("saves on Enter but not while composing, and keeps a stale edit open with the reason", async () => {
		const onSave = vi.fn(async (): Promise<RpcMemoryEditOutcome> => ({ ok: false, reason: "not_found" }));
		await mount({ onSave, onDelete: vi.fn(async () => null) });

		await call(button("Edit"), "onClick");
		const editor = container.querySelector("textarea");
		await call(editor, "onChange", { target: { value: "  Use bun everywhere  " } });
		await call(editor, "onKeyDown", enter(true));
		expect(onSave).not.toHaveBeenCalled();

		await call(editor, "onKeyDown", enter());
		expect(onSave).toHaveBeenCalledWith("Use bun everywhere");
		expect(container.querySelector("textarea")).not.toBeNull();
		expect(container.textContent).toContain("This changed since you opened it");

		onSave.mockResolvedValueOnce({ ok: true });
		await call(container.querySelector("textarea"), "onKeyDown", enter());
		expect(container.querySelector("textarea")).toBeNull();
	});

	it("deletes only on the second click", async () => {
		const onDelete = vi.fn(async (): Promise<RpcMemoryEditOutcome> => ({ ok: true }));
		await mount({ onSave: vi.fn(async () => null), onDelete });

		await call(button("Delete"), "onClick");
		expect(onDelete).not.toHaveBeenCalled();
		await call(button("Delete?"), "onClick");
		expect(onDelete).toHaveBeenCalledTimes(1);
	});
});
