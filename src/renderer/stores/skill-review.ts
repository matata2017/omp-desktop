/**
 * Review state for skills omp learned on its own (the omp-managed provider).
 *
 * Learned skills are active the moment they are written; the person reviews
 * them afterwards. The GUI remembers what each skill looked like when it was
 * last reviewed, so a new skill reads "new" and a rewritten one shows what
 * changed. The snapshot lives in GUI prefs — it is the reader's bookmark, not
 * agent state.
 */
import { createTwoFilesPatch } from "diff";
import { create } from "zustand";
import type { RpcSkillDetail, RpcSkillInfo } from "../../shared/rpc-types";

const PREFS_KEY = "skillReview";

/** The skill as it read when the person last reviewed (or wrote) it. */
export interface ReviewedSkill {
	modifiedAt: number;
	description: string;
	body: string;
	reviewedAt: number;
}

export type SkillReviewState = "new" | "changed" | "reviewed";

/** A learned skill is new until reviewed, and changed when written again after that. */
export function skillReviewState(skill: RpcSkillInfo, reviewed: ReviewedSkill | undefined): SkillReviewState {
	if (!reviewed) return "new";
	if (skill.modifiedAt !== undefined && skill.modifiedAt > reviewed.modifiedAt) return "changed";
	return "reviewed";
}

/** Skills omp wrote by itself; authored and plugin skills are never up for review. */
export function learnedSkillsOf(skills: readonly RpcSkillInfo[]): RpcSkillInfo[] {
	return skills.filter(skill => skill.managed);
}

export function pendingReviewCount(
	learned: readonly RpcSkillInfo[],
	reviewed: Readonly<Record<string, ReviewedSkill>>,
): number {
	let count = 0;
	for (const skill of learned) if (skillReviewState(skill, reviewed[skill.name]) !== "reviewed") count++;
	return count;
}

/** A skill as one reviewable document: its description line, then its instructions. */
function reviewText(skill: { description: string; body: string }): string {
	return `${skill.description.trim()}\n\n${skill.body.trim()}\n`;
}

/** What changed since the last review, as a unified diff; null when the content is the same. */
export function skillReviewDiff(reviewed: ReviewedSkill, detail: RpcSkillDetail): string | null {
	const before = reviewText(reviewed);
	const after = reviewText(detail);
	if (before === after) return null;
	const patch = createTwoFilesPatch("SKILL.md", "SKILL.md", before, after, undefined, undefined, { context: 3 });
	// Hunks only: the file header lines would render as content.
	return patch.slice(patch.indexOf("@@"));
}

function validReviewed(value: unknown): Record<string, ReviewedSkill> {
	if (!value || typeof value !== "object") return {};
	const result: Record<string, ReviewedSkill> = {};
	for (const [name, entry] of Object.entries(value as Record<string, unknown>)) {
		if (!entry || typeof entry !== "object") continue;
		const item = entry as Partial<ReviewedSkill>;
		if (typeof item.modifiedAt !== "number" || typeof item.body !== "string") continue;
		result[name] = {
			modifiedAt: item.modifiedAt,
			description: typeof item.description === "string" ? item.description : "",
			body: item.body,
			reviewedAt: typeof item.reviewedAt === "number" ? item.reviewedAt : 0,
		};
	}
	return result;
}

interface SkillReviewStore {
	hydrated: boolean;
	reviewed: Record<string, ReviewedSkill>;
	/** Learned skills as of the last refresh; null before the first one. */
	learned: RpcSkillInfo[] | null;
	hydrate: () => Promise<void>;
	setLearned: (skills: readonly RpcSkillInfo[]) => void;
	/** Record `detail` as reviewed: it stops counting until omp rewrites it. */
	markReviewed: (detail: RpcSkillDetail) => Promise<void>;
	/** Drop a removed skill's snapshot so a future skill of that name reads new. */
	forget: (name: string) => Promise<void>;
}

async function persist(reviewed: Record<string, ReviewedSkill>): Promise<void> {
	try {
		await window.omp.prefs.set(PREFS_KEY, reviewed);
	} catch {
		// The bookmark is a convenience: a lost write re-shows a skill as unreviewed.
	}
}

export const useSkillReviewStore = create<SkillReviewStore>()((set, get) => ({
	hydrated: false,
	reviewed: {},
	learned: null,

	hydrate: async () => {
		if (get().hydrated) return;
		try {
			set({ reviewed: validReviewed(await window.omp.prefs.get(PREFS_KEY)), hydrated: true });
		} catch {
			set({ hydrated: true });
		}
	},

	setLearned: skills => set({ learned: learnedSkillsOf(skills) }),

	markReviewed: async detail => {
		const current = get().reviewed[detail.name];
		if (current && current.modifiedAt === detail.modifiedAt && current.body === detail.body) return;
		const reviewed = {
			...get().reviewed,
			[detail.name]: {
				modifiedAt: detail.modifiedAt ?? Date.now(),
				description: detail.description,
				body: detail.body,
				reviewedAt: Date.now(),
			},
		};
		set({ reviewed });
		await persist(reviewed);
	},

	forget: async name => {
		if (!(name in get().reviewed)) return;
		const { [name]: _removed, ...reviewed } = get().reviewed;
		set({ reviewed });
		await persist(reviewed);
	},
}));

/** How many learned skills wait for review; 0 until both prefs and the skill list have loaded. */
export function usePendingSkillReviews(): number {
	return useSkillReviewStore(state =>
		state.hydrated && state.learned ? pendingReviewCount(state.learned, state.reviewed) : 0,
	);
}
