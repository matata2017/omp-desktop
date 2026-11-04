/**
 * Learned-skill review: a skill is new until reviewed, reads "updated" only when
 * omp rewrote it after the review, and the review diff shows the change itself.
 */
import { describe, expect, it } from "vitest";
import type { RpcSkillDetail, RpcSkillInfo } from "../../shared/rpc-types";
import {
	learnedSkillsOf,
	pendingReviewCount,
	type ReviewedSkill,
	skillReviewDiff,
	skillReviewState,
} from "./skill-review";

function skill(name: string, modifiedAt: number, managed = true): RpcSkillInfo {
	return {
		name,
		description: "Deploy the app",
		source: "omp:user",
		enabled: true,
		location: `/skills/${name}`,
		provider: "omp",
		providerName: "omp",
		level: "user",
		managed,
		hidden: false,
		modifiedAt,
	};
}

const reviewedAt100: ReviewedSkill = {
	modifiedAt: 100,
	description: "Deploy the app",
	body: "1. build\n2. push\n",
	reviewedAt: 150,
};

describe("skill review", () => {
	it("counts unreviewed and rewritten learned skills, never authored ones", () => {
		const skills = [skill("deploy", 100), skill("release", 200), skill("fresh", 300), skill("mine", 400, false)];
		const reviewed = { deploy: reviewedAt100, release: { ...reviewedAt100, modifiedAt: 100 } };
		const learned = learnedSkillsOf(skills);

		expect(
			learned.map(item => [item.name, skillReviewState(item, reviewed[item.name as keyof typeof reviewed])]),
		).toEqual([
			["deploy", "reviewed"],
			["release", "changed"],
			["fresh", "new"],
		]);
		expect(pendingReviewCount(learned, reviewed)).toBe(2);
	});

	it("diffs the rewrite against what was reviewed, and finds nothing when only the timestamp moved", () => {
		const rewritten: RpcSkillDetail = { ...skill("deploy", 300), body: "1. build\n2. test\n3. push\n" };
		const diff = skillReviewDiff(reviewedAt100, rewritten);
		// Hunks only — DiffView renders any file header as content.
		expect(diff?.startsWith("@@")).toBe(true);
		expect(diff).toContain("+2. test");
		expect(diff).not.toContain("-1. build");

		const touched: RpcSkillDetail = { ...skill("deploy", 300), body: "1. build\n2. push" };
		expect(skillReviewDiff(reviewedAt100, touched)).toBeNull();
	});
});
