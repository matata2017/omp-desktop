import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/**
 * Heavy renderer vendor libs split out of the eager main chunk. Patterns match node_modules path
 * segments; each family (and its transitive deps) lands in a named chunk that lazy overlays load
 * on demand. Copied verbatim from the upstream `electron.vite.config.ts`.
 */
const VENDOR_CHUNK_RULES: ReadonlyArray<readonly [RegExp, string]> = [
	// React core: every chunk depends on it, so shared interop helpers land here — keeping
	// cross-chunk imports one-way (no circular chunks).
	[/[\\/]node_modules[\\/](react|react-dom|scheduler)[\\/]/, "react"],
	// Markdown pipeline: react-markdown + the unified/remark/rehype/mdast/hast ecosystem.
	[
		/[\\/]node_modules[\\/](react-markdown|lowlight|remark-|rehype-|mdast-|hast-|hastscript|unist-|unified|micromark|vfile|devlop|trough|bail|extend|zwitch|ccount|comma-separated-tokens|space-separated-tokens|decode-named-character-reference|character-entities|property-information|html-url-attributes|trim-lines|markdown-table|longest-streak|is-plain-obj|estree-util-|estree-walker|stringify-entities|web-namespaces)/,
		"markdown",
	],
	// KaTeX (pulled in by rehype-katex; kept separate so the markdown chunk stays lean).
	[/[\\/]node_modules[\\/]katex[\\/]/, "katex"],
	// Mermaid + its rendering stack (lazy-imported).
	[
		/[\\/]node_modules[\\/](mermaid|@mermaid-js|d3[^\\/]*|dagre[^\\/]*|@dagrejs|elkjs|dompurify|khroma|cytoscape|stylis|ts-dedent|marked|lodash-es|dayjs|uuid|robust-predicates|topojson-client)[\\/]/,
		"mermaid",
	],
	// chart.js + react-chartjs-2.
	[/[\\/]node_modules[\\/](chart\.js|react-chartjs-2|@kurkle)[\\/]/, "charts"],
	// xterm terminal.
	[/[\\/]node_modules[\\/]@xterm[\\/]/, "xterm"],
	// CodeMirror editor.
	[/[\\/]node_modules[\\/](@codemirror|@lezer)[\\/]/, "codemirror"],
	// highlight.js core + common grammars.
	[/[\\/]node_modules[\\/]highlight\.js[\\/]/, "highlight"],
	// dnd-kit drag and drop.
	[/[\\/]node_modules[\\/]@dnd-kit[\\/]/, "dnd-kit"],
];

function manualChunks(id: string): string | undefined {
	if (!id.includes("node_modules")) return undefined;
	for (const [pattern, chunk] of VENDOR_CHUNK_RULES) {
		if (pattern.test(id)) return chunk;
	}
	return undefined;
}

export default defineConfig({
	root: resolve(__dirname, "src/renderer"),
	plugins: [react(), tailwindcss()],
	resolve: {
		alias: {
			"@renderer": resolve(__dirname, "src/renderer"),
			"@shared": resolve(__dirname, "src/shared"),
		},
	},
	server: {
		port: 1420,
		strictPort: true,
	},
	build: {
		outDir: resolve(__dirname, "dist"),
		emptyOutDir: true,
		target: "chrome110",
		rollupOptions: {
			input: { index: resolve(__dirname, "src/renderer/index.html") },
			output: { manualChunks },
		},
	},
});
