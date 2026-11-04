/**
 * Keyboard chords as the GUI keymap spells them: modifiers ⌃⌥⇧⌘ plus a
 * canonical base key, serialized in the fixed order ⌥ ⇧ ⌃ ⌘. Shared so the
 * main process can recognize app shortcuts typed into a built-in browser page,
 * whose keystrokes never reach the renderer.
 */

export interface Chord {
	ctrl: boolean;
	alt: boolean;
	shift: boolean;
	meta: boolean;
	/** Canonical base key: uppercase letter, digit, literal punctuation, glyph (↑↓←→↵⇥␣⎋⌫⌦), or F-key. */
	key: string;
}

/** Canonical chord string: modifiers in ⌥⇧⌃⌘ order + the canonical base key. */
export function serializeChord(chord: Chord): string {
	return `${chord.alt ? "⌥" : ""}${chord.shift ? "⇧" : ""}${chord.ctrl ? "⌃" : ""}${chord.meta ? "⌘" : ""}${chord.key}`;
}

/** Structural subset of KeyboardEvent that chord extraction reads (test-friendly). */
export interface KeyEventLike {
	key: string;
	code: string;
	ctrlKey: boolean;
	altKey: boolean;
	shiftKey: boolean;
	metaKey: boolean;
}

const IGNORED_EVENT_KEYS: Record<string, true> = {
	Control: true,
	Shift: true,
	Alt: true,
	Meta: true,
	CapsLock: true,
	Fn: true,
	NumLock: true,
	ScrollLock: true,
	Dead: true,
};

const EVENT_KEY_GLYPHS: Record<string, string> = {
	ArrowUp: "↑",
	ArrowDown: "↓",
	ArrowLeft: "←",
	ArrowRight: "→",
	Enter: "↵",
	Tab: "⇥",
	" ": "␣",
	Escape: "⎋",
	Backspace: "⌫",
	Delete: "⌦",
};

/** Physical-code → base key for punctuation (layout-independent, immune to ⌥ composition). */
const CODE_BASE_KEYS: Record<string, string> = {
	Minus: "-",
	Equal: "=",
	BracketLeft: "[",
	BracketRight: "]",
	Backslash: "\\",
	Semicolon: ";",
	Quote: "'",
	Backquote: "`",
	Comma: ",",
	Period: ".",
	Slash: "/",
};

function keyFromEvent(event: KeyEventLike): string | null {
	if (IGNORED_EVENT_KEYS[event.key]) return null;
	const glyph = EVENT_KEY_GLYPHS[event.key];
	if (glyph) return glyph;
	if (/^F(?:[1-9]|1[0-2])$/.test(event.key)) return event.key;
	// Prefer the physical code for letters/digits/punctuation: with ⌥ held,
	// macOS turns event.key into a composition character (⌥R → "®"), and the
	// pre-B3 hardcoded chains were already code-based for exactly this reason.
	if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3);
	if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
	const punct = CODE_BASE_KEYS[event.code];
	if (punct) return punct;
	if (event.key.length === 1) return /^[a-z]$/i.test(event.key) ? event.key.toUpperCase() : event.key;
	return null;
}

/** Chord for a keydown event, or null for pure modifiers / unmodified / shift-only keys. */
export function eventToChord(event: KeyEventLike): Chord | null {
	const key = keyFromEvent(event);
	if (!key) return null;
	if (!event.ctrlKey && !event.altKey && !event.metaKey) return null;
	return { ctrl: event.ctrlKey, alt: event.altKey, shift: event.shiftKey, meta: event.metaKey, key };
}

/** Canonical chord string for a keydown event — the compiled-map lookup key. */
export function chordFromEvent(event: KeyEventLike): string | null {
	const chord = eventToChord(event);
	return chord ? serializeChord(chord) : null;
}
