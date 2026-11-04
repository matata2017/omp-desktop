import { describe, expect, it } from "vitest";
import { parseLogLine } from "./log-line";

describe("parseLogLine", () => {
	it("classifies a JSON record by its level field, not by words inside its fields", () => {
		// A debug record whose fields mention errors used to land in the Error filter.
		const line = parseLogLine(
			'{"timestamp":"2026-10-12T16:22:56.585+08:00","level":"debug","pid":1,"message":"retry routing","lastError":"error: socket closed"}',
		);
		expect(line).toEqual({
			level: "other",
			levelName: "debug",
			time: "16:22:56",
			message: "retry routing",
			fields: "lastError=error: socket closed",
		});
		expect(parseLogLine('{"level":"warn","message":"slow"}').level).toBe("warn");
	});

	it("keeps plain text whole and falls back to keyword levels", () => {
		expect(parseLogLine("[main] ERROR failed to spawn")).toEqual({
			level: "error",
			message: "[main] ERROR failed to spawn",
		});
		// Brace-wrapped text that is not JSON is still just text.
		expect(parseLogLine("{not json}")).toEqual({ level: "other", message: "{not json}" });
	});
});
