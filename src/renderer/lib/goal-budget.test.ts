import { describe, expect, it } from "vitest";
import { formatBudgetDraft, parseBudgetDraft } from "./goal-budget";

describe("parseBudgetDraft", () => {
	it("treats an empty draft as no budget", () => {
		expect(parseBudgetDraft("")).toEqual({ tokens: null, invalid: false });
		expect(parseBudgetDraft("   ")).toEqual({ tokens: null, invalid: false });
	});

	it("accepts whole numbers with common separators", () => {
		expect(parseBudgetDraft("50000")).toEqual({ tokens: 50000, invalid: false });
		expect(parseBudgetDraft("50,000")).toEqual({ tokens: 50000, invalid: false });
		expect(parseBudgetDraft("50_000")).toEqual({ tokens: 50000, invalid: false });
		expect(parseBudgetDraft(" 50 000 ")).toEqual({ tokens: 50000, invalid: false });
	});

	it("rejects zero, negatives, fractions and words", () => {
		for (const raw of ["0", "-5", "1.5", "ten", "1e9"]) {
			expect(parseBudgetDraft(raw), raw).toEqual({ tokens: null, invalid: true });
		}
	});
});

describe("formatBudgetDraft", () => {
	it("round-trips a saved budget and hides a missing one", () => {
		expect(formatBudgetDraft(50000)).toBe("50000");
		expect(formatBudgetDraft(null)).toBe("");
		expect(formatBudgetDraft(undefined)).toBe("");
		expect(formatBudgetDraft(0)).toBe("");
	});
});
