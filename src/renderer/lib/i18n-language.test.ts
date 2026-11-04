import { afterEach, describe, expect, it, vi } from "vitest";
import { getCurrentLanguage, translate } from "./i18n";

afterEach(() => vi.unstubAllGlobals());

describe("Russian language preference", () => {
	it("detects Russian from the system unless a language was explicitly saved", () => {
		vi.stubGlobal("navigator", { language: "ru-RU" });
		vi.stubGlobal("localStorage", { getItem: () => null });
		expect(getCurrentLanguage()).toBe("ru");
		vi.stubGlobal("localStorage", { getItem: () => "en" });
		expect(getCurrentLanguage()).toBe("en");
	});
	it("uses Russian when explicitly selected on an English system", () => {
		vi.stubGlobal("localStorage", { getItem: () => "ru" });
		expect(getCurrentLanguage()).toBe("ru");
		expect(translate("common.save")).toBe("Сохранить");
	});
	it("replaces every repeated placeholder and preserves literal dollar signs", () => {
		vi.stubGlobal("localStorage", { getItem: () => "ru" });
		const text = translate("modelRestore.offer", { model: "$&" });
		expect(text).not.toContain("{model}");
		expect(text.match(/\$&/g)).toHaveLength(2);
	});
});
