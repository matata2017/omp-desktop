import { type RefObject, useEffect, useRef, type WheelEvent } from "react";

/** A plain mouse wheel only scrolls vertically; map it onto a sideways chip strip. */
export function scrollStripOnWheel(event: WheelEvent<HTMLElement>): void {
	if (Math.abs(event.deltaY) > Math.abs(event.deltaX)) event.currentTarget.scrollLeft += event.deltaY;
}

/** Scroll a chip into its strip's view when it becomes the active one (an overflowing strip hides its ends). */
export function useActiveChipInView<T extends HTMLElement>(active: boolean): RefObject<T | null> {
	const ref = useRef<T>(null);
	useEffect(() => {
		if (active) ref.current?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
	}, [active]);
	return ref;
}
