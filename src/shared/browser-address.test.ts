import { describe, expect, it } from "vitest";
import { chromeUserAgent, isPageUrl, normalizeAddress } from "./browser-address";

describe("normalizeAddress", () => {
	it("routes what the user typed to a URL: https for hosts, http for local dev servers, file for paths", () => {
		expect(normalizeAddress("example.com/docs?q=1")).toBe("https://example.com/docs?q=1");
		expect(normalizeAddress("localhost:5173")).toBe("http://localhost:5173");
		expect(normalizeAddress("127.0.0.1:8080/api")).toBe("http://127.0.0.1:8080/api");
		expect(normalizeAddress("/Users/me/report.html")).toBe("file:///Users/me/report.html");
		expect(normalizeAddress("  https://example.com  ")).toBe("https://example.com");
	});

	it("searches for anything that is not an address", () => {
		expect(normalizeAddress("how to center a div")).toBe("https://duckduckgo.com/?q=how%20to%20center%20a%20div");
		expect(normalizeAddress("typescript")).toBe("https://duckduckgo.com/?q=typescript");
	});
});

describe("isPageUrl", () => {
	it("refuses schemes that run code or reach outside the page", () => {
		expect(isPageUrl("https://example.com")).toBe(true);
		expect(isPageUrl("about:blank")).toBe(true);
		expect(isPageUrl("javascript:alert(1)")).toBe(false);
		expect(isPageUrl("chrome://settings")).toBe(false);
		expect(isPageUrl("vscode://file/etc/passwd")).toBe(false);
	});
});

describe("chromeUserAgent", () => {
	it("drops the Electron and app tokens sites use to reject embedded browsers", () => {
		const electron =
			"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) omp/0.9.20 Chrome/136.0.7103.177 Electron/36.9.5 Safari/537.36";
		expect(chromeUserAgent(electron, "omp")).toBe(
			"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.7103.177 Safari/537.36",
		);
	});
});
