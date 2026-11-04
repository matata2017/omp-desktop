/**
 * AdvisorBadge contract (TUI status-line parity): hidden unless advisors are
 * enabled and configured; colored by the worst roster status; the eye closes
 * once every advisor has yielded; spend shows once there is any.
 */
import { parseHTML } from "linkedom";
import { act, type ReactElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import type { RpcAdvisorStatus } from "../../../shared/rpc-types";
import { I18nProvider } from "../../lib/i18n";
import { useSessionStore } from "../../stores/session";
import { AdvisorBadge, advisorBadgeColor } from "./AdvisorBadge";

const { document, window, HTMLElement, Element, Node } = parseHTML("<html><body></body></html>");
const globals = globalThis as Record<string, unknown>;
Object.assign(globals, { document, window, HTMLElement, Element, Node, IS_REACT_ACT_ENVIRONMENT: true });

interface TestElement {
	textContent: string | null;
	remove: () => void;
	getAttribute: (name: string) => string | null;
	querySelector: (selector: string) => TestElement | null;
}

let container: TestElement | undefined;
let root: Root | undefined;

async function mount(element: ReactElement): Promise<TestElement> {
	const host = document.createElement("div") as unknown as TestElement;
	document.body.appendChild(host as never);
	const created = createRoot(host as unknown as Element);
	container = host;
	root = created;
	await act(async () => {
		created.render(<I18nProvider>{element}</I18nProvider>);
	});
	return host;
}

afterEach(async () => {
	const mounted = root;
	if (mounted) {
		await act(async () => {
			mounted.unmount();
		});
	}
	container?.remove();
	root = undefined;
	container = undefined;
	useSessionStore.getState().reset();
});

const status = (patch: Partial<RpcAdvisorStatus>): RpcAdvisorStatus => ({
	configured: true,
	advisors: [{ name: "watchdog", status: "running", yielded: false }],
	cost: 0,
	usingSubscription: false,
	...patch,
});

describe("advisorBadgeColor", () => {
	it("ranks error over quota exhaustion over running", () => {
		expect(advisorBadgeColor(["running", "error", "quota_exhausted"])).toBe("var(--omp-error)");
		expect(advisorBadgeColor(["running", "quota_exhausted"])).toBe("var(--omp-warning)");
		expect(advisorBadgeColor(["paused", "running"])).toBe("var(--omp-success)");
		expect(advisorBadgeColor(["paused", "no_model"])).toBe("var(--omp-dim)");
	});
});

describe("AdvisorBadge", () => {
	it("stays hidden when advisors are off or none are configured", async () => {
		useSessionStore.setState({ advisorStatus: status({ configured: false }) });
		const host = await mount(<AdvisorBadge />);
		expect(host.querySelector("button")).toBeNull();
		await act(async () => {
			useSessionStore.setState({ advisorStatus: status({ advisors: [] }) });
		});
		expect(host.querySelector("button")).toBeNull();
	});

	it("lists each advisor's status and the spend in its title", async () => {
		useSessionStore.setState({
			advisorStatus: status({
				advisors: [
					{ name: "watchdog", status: "running", yielded: true },
					{ name: "security", status: "quota_exhausted", yielded: true },
				],
				cost: 0.0812,
			}),
		});
		const host = await mount(<AdvisorBadge />);
		const title = host.querySelector("button")?.getAttribute("title") ?? "";
		expect(title).toContain("Advisor finished reviewing");
		expect(title).toContain("watchdog: active");
		expect(title).toContain("security: quota exhausted");
		expect(title).toContain("Advisor cost: $0.0812");
		expect(host.textContent).toContain("$0.08");
	});
});
