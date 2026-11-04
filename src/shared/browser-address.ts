/**
 * Address-bar and navigation rules for the built-in browser, shared by main
 * (which navigates) and the renderer (which shows what will happen).
 */

/** Schemes a page may load at the top level. Everything else is refused. */
const PAGE_SCHEMES = new Set(["http:", "https:", "file:", "about:"]);

const SEARCH_URL = "https://duckduckgo.com/?q=";

/** True when `url` may be loaded as a page (no `javascript:`, `chrome:`, custom app schemes). */
export function isPageUrl(url: string): boolean {
	try {
		return PAGE_SCHEMES.has(new URL(url).protocol);
	} catch {
		return false;
	}
}

/**
 * Turn what the user typed into a URL: full URLs pass through, a host (with an
 * optional port or path) gets `https://` — `http://` for localhost and IP
 * literals, where dev servers live — an absolute path becomes `file://`, and
 * anything else is a web search.
 */
export function normalizeAddress(input: string): string {
	const text = input.trim();
	if (!text) return "about:blank";
	if (/^[a-z][a-z\d+.-]*:\/\//i.test(text) || /^about:/i.test(text)) return text;
	if (text.startsWith("/")) return `file://${encodeURI(text)}`;
	if (!/\s/.test(text)) {
		const host = text.split(/[/?#]/, 1)[0] ?? "";
		const hostname = host.replace(/:\d+$/, "");
		const local = hostname === "localhost" || /^\d{1,3}(?:\.\d{1,3}){3}$/.test(hostname) || hostname === "[::1]";
		if (local) return `http://${text}`;
		if (/^[\w-]+(?:\.[\w-]+)+$/.test(hostname)) return `https://${text}`;
	}
	return `${SEARCH_URL}${encodeURIComponent(text)}`;
}

/** Chrome's user agent without the Electron and app tokens that sites gate as embedded browsers. */
export function chromeUserAgent(userAgent: string, appName: string): string {
	const escaped = appName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
	return userAgent
		.replace(/ Electron\/\S+/, "")
		.replace(new RegExp(` ${escaped}\\/\\S+`), "")
		.replace(/\s{2,}/g, " ");
}
