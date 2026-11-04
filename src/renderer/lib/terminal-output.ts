/**
 * Terminal output ordering. Every sidecar `terminal_output` frame carries
 * `seq`, the count of characters the terminal has produced through the end of
 * its `data`; a `terminal_snapshot` carries the scrollback through its own
 * `seq`. A view that loaded a snapshot, or already wrote a frame, must write
 * only what comes after it, or output races (snapshot vs. live frames) print
 * twice.
 */

/** The part of `frame` a view showing output through `shownSeq` has not written yet. */
export function unseenOutput(shownSeq: number, frame: { data: string; seq: number }): string {
	if (frame.seq <= shownSeq) return "";
	const start = frame.seq - frame.data.length;
	return start >= shownSeq ? frame.data : frame.data.slice(shownSeq - start);
}
