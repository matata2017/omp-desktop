/**
 * Goal budget input: the panel accepts a token budget as free text, an empty
 * field means "no budget" (the sidecar only enforces one when present).
 */
export interface BudgetDraft {
	/** null = no budget, undefined never returned: invalid input keeps the old value. */
	tokens: number | null;
	invalid: boolean;
}

export function parseBudgetDraft(raw: string): BudgetDraft {
	const cleaned = raw.replace(/[_,\s]/g, "");
	if (cleaned === "") return { tokens: null, invalid: false };
	if (!/^\d+$/.test(cleaned)) return { tokens: null, invalid: true };
	const tokens = Number(cleaned);
	if (!Number.isSafeInteger(tokens) || tokens <= 0) return { tokens: null, invalid: true };
	return { tokens, invalid: false };
}

export function formatBudgetDraft(tokens: number | null | undefined): string {
	return typeof tokens === "number" && tokens > 0 ? String(tokens) : "";
}
