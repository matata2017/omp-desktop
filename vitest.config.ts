import { defineConfig } from "vitest/config";

export default defineConfig({
	test: {
		include: ["src/**/*.test.{ts,tsx}", "scripts/**/*.test.ts"],
		setupFiles: ["./scripts/test-locale.ts"],
		// main process is Rust; this test imports src/main/workspace-paths (see docs/parity.md).
		exclude: ["src/renderer/lib/markdown-links.test.tsx"],
	},
});
