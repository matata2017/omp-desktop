import { describe, expect, it } from "vitest";
import { unseenOutput } from "./terminal-output";

describe("unseenOutput", () => {
	it("drops a frame the snapshot already contained and keeps only the tail of one it overlapped", () => {
		// Snapshot through seq 10; frames ending at 8 and 14 raced it.
		expect(unseenOutput(10, { data: "abcdefgh", seq: 8 })).toBe("");
		expect(unseenOutput(10, { data: "ghijklmn", seq: 14 })).toBe("klmn");
	});

	it("writes a later frame whole, including after a gap", () => {
		expect(unseenOutput(10, { data: "klm", seq: 13 })).toBe("klm");
		expect(unseenOutput(10, { data: "xyz", seq: 40 })).toBe("xyz");
	});
});
