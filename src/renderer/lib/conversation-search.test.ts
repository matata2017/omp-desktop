/**
 * Snippets arrive with match markers from the main-process index; the dialog
 * must highlight exactly the matched runs and never print a marker character.
 */
import { describe, expect, it } from "vitest";
import { snippetParts } from "./conversation-search";

describe("snippetParts", () => {
	it("splits marked matches from the text around them", () => {
		expect(snippetParts("…why \u0001notarize\u0002 and \u0001sign\u0002 fail…")).toEqual([
			{ text: "…why ", match: false },
			{ text: "notarize", match: true },
			{ text: " and ", match: false },
			{ text: "sign", match: true },
			{ text: " fail…", match: false },
		]);
	});

	it("drops unbalanced markers instead of showing them", () => {
		const parts = snippetParts("\u0002cut \u0001open to the end");
		expect(parts).toEqual([
			{ text: "cut ", match: false },
			{ text: "open to the end", match: true },
		]);
		expect(parts.map(part => part.text).join("")).not.toMatch(/[\u0001\u0002]/);
	});
});
