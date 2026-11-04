import { describe, expect, it } from "vitest";
import type { SettingEntry } from "../../../shared/rpc-types";
import { schemaGroupTitle, schemaOptionText, settingTranslation } from "./schema-localization";
import { matchesSettingSearch } from "./settings-schema-utils";

describe("Russian schema localization", () => {
	it("localizes labels, descriptions, groups and enum prose", () => {
		expect(settingTranslation("ru", "update.channel")?.label).toMatch(/[А-Яа-яЁё]/);
		expect(settingTranslation("ru", "update.channel")?.description).toMatch(/[А-Яа-яЁё]/);
		expect(schemaGroupTitle("ru", "Skills")).toBe("Skills");
		expect(schemaOptionText("ru", "Stable")).toMatch(/[А-Яа-яЁё]/);
		expect(schemaOptionText("ru", "unregistered-enum-value")).toBe("unregistered-enum-value");
	});
	it("preserves English wire text and existing Chinese translations", () => {
		expect(settingTranslation("en", "update.channel")).toBeUndefined();
		expect(schemaGroupTitle("en", "Display")).toBe("Display");
		expect(schemaGroupTitle("zh", "Display")).toBe("显示");
	});
	it("searches Russian displayed setting text alongside wire identifiers", () => {
		const entry = { path: "update.channel", label: "Core Update Channel", type: "enum" } as SettingEntry;
		const label = settingTranslation("ru", entry.path)?.label ?? "";
		expect(label).not.toBe("");
		expect(matchesSettingSearch(entry, label)).toBe(true);
	});
});
