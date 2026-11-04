import { describe, expect, it } from "vitest";
import { findTextLinks } from "./terminal-links";

const urls = (text: string) => findTextLinks(text).map(link => link.url);

describe("findTextLinks", () => {
	it("finds the addresses a dev server prints, with their positions", () => {
		const line = "  ➜  Local:   http://localhost:5173/   Network: http://192.168.1.4:5173/";
		const links = findTextLinks(line);
		expect(links.map(link => link.url)).toEqual(["http://localhost:5173/", "http://192.168.1.4:5173/"]);
		expect(line.slice(links[0]?.start, links[0]?.end)).toBe("http://localhost:5173/");
	});

	it("leaves out prose punctuation and brackets the address did not open", () => {
		expect(urls("Ready on https://example.com/app.")).toEqual(["https://example.com/app"]);
		expect(urls("(see https://example.com/docs)")).toEqual(["https://example.com/docs"]);
		expect(urls("https://en.wikipedia.org/wiki/Bun_(software)")).toEqual([
			"https://en.wikipedia.org/wiki/Bun_(software)",
		]);
		expect(urls("open 'https://example.com/a?b=1'")).toEqual(["https://example.com/a?b=1"]);
	});

	it("ignores schemes the panel cannot open and bare scheme prefixes", () => {
		expect(urls("file:///etc/hosts ftp://example.com http://")).toEqual([]);
	});
});
