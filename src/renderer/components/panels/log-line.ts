/**
 * One application log line, read for display. The sidecar and main process
 * write JSON records (`{"timestamp","level","message",…}`); anything else is
 * plain text. Levels come from the record's own `level` field — scanning the
 * text for "error" would flag every debug record whose fields mention one.
 */

export type LogLevel = "info" | "warn" | "error" | "other";

export interface ParsedLogLine {
	level: LogLevel;
	/** The record's level as written (debug, info, …); absent for plain text. */
	levelName?: string;
	/** Wall-clock HH:MM:SS of the record, when it carries a timestamp. */
	time?: string;
	message: string;
	/** Remaining record fields as `key=value` pairs. */
	fields?: string;
}

const RESERVED_KEYS = new Set(["timestamp", "time", "ts", "level", "message", "msg", "pid"]);
const FIELD_VALUE_CHARS = 160;

function levelFromName(name: string): LogLevel {
	const lower = name.toLowerCase();
	if (lower === "error" || lower === "fatal" || lower === "critical") return "error";
	if (lower === "warn" || lower === "warning") return "warn";
	if (lower === "info" || lower === "notice") return "info";
	return "other";
}

function levelFromText(text: string): LogLevel {
	const lower = text.toLowerCase();
	if (/\b(error|err!|fatal)\b/.test(lower)) return "error";
	if (/\b(warn|warning)\b/.test(lower)) return "warn";
	if (/\binfo\b/.test(lower)) return "info";
	return "other";
}

function clockOf(value: unknown): string | undefined {
	if (typeof value !== "string" && typeof value !== "number") return undefined;
	if (typeof value === "string") {
		const iso = /T(\d{2}:\d{2}:\d{2})/.exec(value);
		if (iso) return iso[1];
	}
	const date = new Date(value);
	return Number.isNaN(date.getTime()) ? undefined : date.toTimeString().slice(0, 8);
}

function fieldValue(value: unknown): string {
	const text = typeof value === "string" ? value : JSON.stringify(value);
	return text.length > FIELD_VALUE_CHARS ? `${text.slice(0, FIELD_VALUE_CHARS)}…` : text;
}

export function parseLogLine(text: string): ParsedLogLine {
	const trimmed = text.trim();
	if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
		try {
			const record = JSON.parse(trimmed) as Record<string, unknown>;
			if (record && typeof record === "object" && !Array.isArray(record)) {
				const levelName = typeof record.level === "string" ? record.level : undefined;
				const rawMessage = record.message ?? record.msg;
				const message =
					typeof rawMessage === "string" ? rawMessage : rawMessage === undefined ? "" : fieldValue(rawMessage);
				const fields = Object.entries(record)
					.filter(([key]) => !RESERVED_KEYS.has(key))
					.map(([key, value]) => `${key}=${fieldValue(value)}`)
					.join("  ");
				return {
					level: levelName ? levelFromName(levelName) : levelFromText(message),
					levelName,
					time: clockOf(record.timestamp ?? record.time ?? record.ts),
					message: message || trimmed,
					fields: fields || undefined,
				};
			}
		} catch {
			// Not a JSON record after all — show it as text.
		}
	}
	return { level: levelFromText(text), message: text };
}
