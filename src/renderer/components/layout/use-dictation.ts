import type { RefObject } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
	DictationSession,
	type DictationView,
	insertDictated,
	openBrowserMic,
	trimBeforeCaret,
} from "../../lib/dictation";
import { useT } from "../../lib/i18n";
import { onEscape } from "../../lib/keymap";
import type { TabRpc } from "../../lib/tab-rpc";
import { useComposerStore } from "../../stores/composer";
import { toast } from "../../stores/toast";

export interface ComposerDictation {
	/** The running capture, or `null`. */
	view: DictationView | null;
	/** Start dictating, or finish the running capture (a start still loading is cancelled). */
	toggle(): void;
	/** Finish: keep what was said. */
	stop(): void;
	/** Discard the capture. */
	cancel(): void;
}

/**
 * Live dictation into the composer textarea: words land at the caret as each phrase ends, and a
 * spoken submit (per `stt.submitTrigger`) sends once the capture is over. Esc discards; the
 * capture also ends with the composer or when its tab changes.
 */
export function useDictation(options: {
	rpc: TabRpc;
	tabId: string | null;
	textareaRef: RefObject<HTMLTextAreaElement | null>;
	send: (text: string) => void;
}): ComposerDictation {
	const { rpc, tabId, textareaRef } = options;
	const t = useT();
	const [view, setView] = useState<DictationView | null>(null);
	const sessionRef = useRef<DictationSession | null>(null);
	/** Where the next words go when the textarea has not re-rendered since the last insert. */
	const caretRef = useRef<number | null>(null);
	const sendRef = useRef(options.send);
	sendRef.current = options.send;

	/** The draft and the caret to edit at. */
	const target = useCallback(() => {
		const draft = useComposerStore.getState().draft;
		const el = textareaRef.current;
		if (el && el.value === draft) {
			const start = el.selectionStart ?? draft.length;
			return { draft, start, end: el.selectionEnd ?? start };
		}
		const caret = Math.min(caretRef.current ?? draft.length, draft.length);
		return { draft, start: caret, end: caret };
	}, [textareaRef]);

	const apply = useCallback(
		(next: { value: string; caret: number }) => {
			caretRef.current = next.caret;
			useComposerStore.getState().setDraft(next.value);
			requestAnimationFrame(() => {
				const el = textareaRef.current;
				if (el && el.value === next.value) el.setSelectionRange(next.caret, next.caret);
			});
		},
		[textareaRef],
	);

	const cancel = useCallback(() => sessionRef.current?.cancel(), []);
	const stop = useCallback(() => sessionRef.current?.stop(), []);

	const toggle = useCallback(() => {
		const running = sessionRef.current;
		if (running?.phase) {
			running.stop();
			return;
		}
		if (!tabId) return;
		caretRef.current = null;
		// Keys belong to the composer while dictating: Enter finishes, Esc discards.
		textareaRef.current?.focus();
		const session = new DictationSession({
			rpc,
			subscribe: listener =>
				window.omp.events.onTabDictationFrame((frame, frameTabId) => {
					if (frameTabId === tabId) listener(frame);
				}),
			openMic: openBrowserMic,
			editor: {
				commit: text => {
					const { draft, start, end } = target();
					apply(insertDictated(draft, start, end, text));
				},
				trim: count => {
					const { draft, start } = target();
					apply(trimBeforeCaret(draft, start, count));
				},
				submit: () => sendRef.current(useComposerStore.getState().draft),
			},
			onView: next => {
				if (sessionRef.current !== session) return;
				setView(next);
				if (next === null) sessionRef.current = null;
			},
			onWarning: message => toast({ variant: "error", title: t("voice.mic.failed"), message }),
			onNothingHeard: () => toast({ variant: "info", message: t("voice.dictation.nothingHeard"), durationMs: 3000 }),
		});
		sessionRef.current = session;
		session.start();
	}, [apply, rpc, t, tabId, target, textareaRef]);

	// Esc discards from anywhere (the mic button keeps focus after a click), and is consumed so
	// the running turn is not aborted with it.
	const active = view !== null;
	useEffect(() => {
		if (!active) return;
		const onKey = (event: KeyboardEvent) => {
			if (!event.defaultPrevented) onEscape(event, cancel);
		};
		document.addEventListener("keydown", onKey);
		return () => document.removeEventListener("keydown", onKey);
	}, [active, cancel]);

	// A capture belongs to its tab and its composer: it ends when either goes.
	useEffect(() => {
		if (!tabId) return;
		return () => sessionRef.current?.cancel();
	}, [tabId]);

	return { view, toggle, stop, cancel };
}
