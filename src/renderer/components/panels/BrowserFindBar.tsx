import { ChevronDown, ChevronUp, Search, X } from "lucide-react";
import type { RefObject } from "react";
import type { IpcBrowserPage } from "../../../shared/ipc-types";
import { cx } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { isImeKeyEvent } from "../../lib/ime";
import { onEscape } from "../../lib/keymap";
import { ToolbarIconButton } from "./workspace-ui";

/** Find in the shown page: the field, the match count, and stepping through matches. */
export function FindBar({
	inputRef,
	text,
	result,
	onSearch,
	onStep,
	onClose,
}: {
	inputRef: RefObject<HTMLInputElement | null>;
	text: string;
	result: IpcBrowserPage["find"];
	/** `composing`: an input method is mid-word, so only the text updates. */
	onSearch: (text: string, composing: boolean) => void;
	onStep: (backward: boolean) => void;
	onClose: () => void;
}) {
	const t = useT();
	return (
		<div className="flex h-9 shrink-0 items-center gap-1 border-b border-(--omp-border-muted) pr-2 pl-3">
			<Search aria-hidden="true" size={13} className="shrink-0 text-(--omp-dim)" />
			<input
				ref={inputRef}
				aria-label={t("browser.find")}
				placeholder={t("browser.find")}
				value={text}
				spellCheck={false}
				autoCapitalize="off"
				autoCorrect="off"
				onChange={event => onSearch(event.target.value, (event.nativeEvent as InputEvent).isComposing)}
				onCompositionEnd={event => onSearch(event.currentTarget.value, false)}
				onKeyDown={event => {
					// Claimed, or App's Escape would stop the agent's turn.
					if (onEscape(event, onClose)) return;
					if (event.key !== "Enter" || isImeKeyEvent(event)) return;
					event.preventDefault();
					onStep(event.shiftKey);
				}}
				className="h-7 min-w-0 flex-1 bg-transparent px-1.5 text-omp-sm text-(--omp-text) outline-none placeholder:text-(--omp-dim)"
			/>
			{text && result && (
				<span
					role="status"
					className={cx(
						"shrink-0 px-1 font-mono text-omp-xs tabular-nums",
						result.matches === 0 ? "text-(--omp-error)" : "text-(--omp-dim)",
					)}
				>
					{t("browser.findCount", { active: result.active, total: result.matches })}
				</span>
			)}
			<ToolbarIconButton label={t("browser.findPrevious")} disabled={!text} onClick={() => onStep(true)}>
				<ChevronUp size={15} />
			</ToolbarIconButton>
			<ToolbarIconButton label={t("browser.findNext")} disabled={!text} onClick={() => onStep(false)}>
				<ChevronDown size={15} />
			</ToolbarIconButton>
			<ToolbarIconButton label={t("browser.findClose")} onClick={onClose}>
				<X size={14} />
			</ToolbarIconButton>
		</div>
	);
}
