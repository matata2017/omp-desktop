import { describe, expect, it } from "vitest";
import { en } from "./en";
import { ru } from "./ru";
import identicalKeys from "./ru-identical.json";

const placeholders = (text: string) =>
	[...text.matchAll(/\{(\w+)\}/g)]
		.map(match => match[1])
		.filter(name => name !== "plural")
		.sort();

describe("Russian locale completeness", () => {
	it("covers exactly the English source keys with nonempty translations", () => {
		expect(Object.keys(ru).sort()).toEqual(Object.keys(en).sort());
		for (const [key, value] of Object.entries(ru)) expect(value.trim(), key).not.toBe("");
	});
	it("preserves every parameter occurrence except the English plural suffix", () => {
		for (const key of Object.keys(en)) expect(placeholders(ru[key]), key).toEqual(placeholders(en[key]));
	});
	it("only leaves reviewed technical names and code in English", () => {
		for (const [key, value] of Object.entries(en)) {
			if (identicalKeys.includes(key)) {
				expect(ru[key], key).toBe(value);
				continue;
			}
			if (!/[a-z]/i.test(value.replace(/\{\w+\}/g, ""))) continue;
			expect(ru[key], key).not.toBe(value);
		}
	});
	it("retains protected technical terminology and named modes", () => {
		for (const [key, value] of Object.entries(en)) {
			if (/\bMCP\b/.test(value)) expect(ru[key], key).toMatch(/\bMCP\b/);
			if (/\bskills?\b/i.test(value)) expect(ru[key], key).toMatch(/\bskills?\b/i);
			if (/\bplugins?\b/i.test(value)) expect(ru[key], key).toMatch(/\bplugins?\b/i);
			if (/\bhooks?\b/i.test(value)) expect(ru[key], key).toMatch(/\bhooks?\b/i);
		}
		expect(ru["tabs.new.agent"]).toContain("Agent");
		expect(ru["tabs.new.chat"]).toContain("Chat");
		expect(ru["tabs.new.agentHint"]).toContain("Agent");
		expect(ru["tabs.new.chatHint"]).toContain("Chat");
		expect(ru["tabs.kind.chat"]).toContain("Chat");
		expect(ru["tabs.kindMismatch"]).toContain("Agent");
		expect(ru["tabs.kindMismatch"]).toContain("Chat");
	});
	it("localizes visible starter templates while preserving their parameters", () => {
		for (const key of Object.keys(en)) {
			if (
				(key.startsWith("chat.starter.") && key.endsWith(".prompt")) ||
				key === "planPanel.approveMessage" ||
				key === "planPanel.reviseMessage"
			) {
				expect(ru[key], key).toMatch(/[А-Яа-яЁё]/);
				expect(placeholders(ru[key]), key).toEqual(placeholders(en[key]));
			}
		}
	});
});
