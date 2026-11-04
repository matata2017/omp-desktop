import type { Language } from "../../../shared/languages";
import { RU_SCHEMA_OPTION_TEXT } from "./schema-option-ru";
import { ZH_SCHEMA_OPTION_TEXT } from "./schema-option-zh";
import { RU_GROUP_TITLES, RU_SETTINGS } from "./schema-ru";
import { ZH_GROUP_TITLES, ZH_SETTINGS } from "./schema-zh";

export function settingTranslation(language: Language, path: string) {
	return language === "ru" ? RU_SETTINGS[path] : language === "zh" ? ZH_SETTINGS[path] : undefined;
}

export function schemaGroupTitle(language: Language, title: string): string {
	const titles = language === "ru" ? RU_GROUP_TITLES : language === "zh" ? ZH_GROUP_TITLES : undefined;
	return titles?.[title] ?? title;
}

export function schemaOptionText(language: Language, text: string): string {
	const options = language === "ru" ? RU_SCHEMA_OPTION_TEXT : language === "zh" ? ZH_SCHEMA_OPTION_TEXT : undefined;
	return options?.[text] ?? text;
}
