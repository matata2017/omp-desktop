/**
 * A request to show one message of a session: set by conversation search, read
 * by the transcript of whichever tab ends up showing that session file. It is
 * window-wide because the message may open in a tab that does not exist yet.
 */
import { create } from "zustand";

/** A request older than this is dropped instead of scrolling a later visit. */
export const TRANSCRIPT_FOCUS_TTL_MS = 60_000;

export interface TranscriptFocusTarget {
	sessionPath: string;
	entryId: string;
	/** Distinguishes repeated requests for the same message. */
	nonce: number;
	requestedAt: number;
}

interface TranscriptFocusState {
	target: TranscriptFocusTarget | null;
	focus: (sessionPath: string, entryId: string) => void;
	/** Done with this request; a newer one stays. */
	settle: (nonce: number) => void;
}

export const useTranscriptFocusStore = create<TranscriptFocusState>((set, get) => ({
	target: null,
	focus: (sessionPath, entryId) =>
		set({
			target: { sessionPath, entryId, nonce: (get().target?.nonce ?? 0) + 1, requestedAt: Date.now() },
		}),
	settle: nonce => {
		if (get().target?.nonce === nonce) set({ target: null });
	},
}));
