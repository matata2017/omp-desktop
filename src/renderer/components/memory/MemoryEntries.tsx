/**
 * Notes and lessons as editable rows: edit in place, delete with a second
 * click, and (for lessons) keep one as a note so it outlives the next rebuild.
 */
import { Check, Lightbulb, Pencil, Pin, Trash2, X } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import type { RpcMemoryEditOutcome, RpcMemoryListEntry } from "../../../shared/rpc-types";
import { cx } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { isImeKeyEvent } from "../../lib/ime";
import { Button, TextArea } from "../common";

/** How long a destructive button stays armed after its first click. */
const ARM_MS = 3000;

/** Two-click confirm for an inline destructive action: first click arms, second fires. */
export function useArmed(): { armed: string | null; arm: (id: string) => boolean } {
	const [armed, setArmed] = useState<string | null>(null);
	useEffect(() => {
		if (!armed) return;
		const timer = window.setTimeout(() => setArmed(null), ARM_MS);
		return () => window.clearTimeout(timer);
	}, [armed]);
	const arm = useCallback(
		(id: string) => {
			if (armed === id) {
				setArmed(null);
				return true;
			}
			setArmed(id);
			return false;
		},
		[armed],
	);
	return { armed, arm };
}

/**
 * Enter commits, Shift+Enter breaks the line, Escape cancels — and none of it
 * fires while an input method is composing.
 */
function editorKeys(commit: () => void, cancel?: () => void) {
	return (event: KeyboardEvent<HTMLTextAreaElement>) => {
		if (isImeKeyEvent(event)) return;
		if (event.key === "Enter" && !event.shiftKey) {
			event.preventDefault();
			commit();
		} else if (event.key === "Escape" && cancel) {
			event.preventDefault();
			cancel();
		}
	};
}

function outcomeMessage(t: (key: string) => string, outcome: RpcMemoryEditOutcome | null): string | null {
	if (!outcome || outcome.ok) return null;
	return t(`memory.edit.${outcome.reason}`);
}

export function AddNote({
	onAdd,
	full,
}: {
	onAdd: (text: string) => Promise<RpcMemoryEditOutcome | null>;
	full: boolean;
}) {
	const t = useT();
	const [text, setText] = useState("");
	const [saving, setSaving] = useState(false);
	const [problem, setProblem] = useState<string | null>(null);
	// Remount after a save so the auto-grown field shrinks back to one line.
	const [generation, setGeneration] = useState(0);
	const ref = useRef<HTMLTextAreaElement | null>(null);

	const commit = async () => {
		if (!text.trim() || saving) return;
		setSaving(true);
		const outcome = await onAdd(text.trim());
		setSaving(false);
		const message = outcomeMessage(t, outcome);
		if (message) {
			setProblem(message);
			return;
		}
		if (outcome?.ok) {
			setText("");
			setProblem(null);
			setGeneration(value => value + 1);
			requestAnimationFrame(() => ref.current?.focus());
		}
	};

	return (
		<div className="flex flex-col gap-1.5">
			<div className="flex items-start gap-2">
				<div className="min-w-0 flex-1">
					<TextArea
						aria-label={t("memory.notes.add")}
						autoGrow
						className="min-h-10 overflow-hidden py-[0.4375rem] leading-6"
						disabled={full || saving}
						key={generation}
						maxLength={1000}
						onChange={event => {
							setText(event.target.value);
							if (problem) setProblem(null);
						}}
						onKeyDown={editorKeys(() => void commit())}
						placeholder={full ? t("memory.notes.full") : t("memory.notes.placeholder")}
						ref={ref}
						rows={1}
						value={text}
					/>
				</div>
				<Button
					className="h-10 shrink-0"
					disabled={!text.trim() || full}
					loading={saving}
					onClick={() => void commit()}
					size="md"
					variant="primary"
				>
					{t("memory.notes.addButton")}
				</Button>
			</div>
			{problem ? (
				<p className="text-omp-xs text-(--omp-warning)">{problem}</p>
			) : (
				text.length > 0 && (
					<p className="text-omp-xs text-(--omp-dim)">
						{t("memory.notes.enterHint")}
						{text.length > 800 && ` · ${text.length}/1000`}
					</p>
				)
			)}
		</div>
	);
}

export interface EntryAction {
	id: string;
	label: string;
	icon: ReactNode;
	onClick: () => void;
}

export function MemoryEntryRow({
	entry,
	kind,
	dimmed,
	marker,
	armed,
	onArm,
	onSave,
	onDelete,
	extraActions = [],
}: {
	entry: RpcMemoryListEntry;
	kind: "note" | "lesson";
	/** Present but not brought into tasks (over the token budget). */
	dimmed?: boolean;
	marker?: ReactNode;
	armed: boolean;
	onArm: () => boolean;
	onSave: (text: string) => Promise<RpcMemoryEditOutcome | null>;
	onDelete: () => Promise<RpcMemoryEditOutcome | null>;
	extraActions?: EntryAction[];
}) {
	const t = useT();
	const [editing, setEditing] = useState(false);
	const [draft, setDraft] = useState(entry.text);
	const [busy, setBusy] = useState(false);
	const [problem, setProblem] = useState<string | null>(null);
	const rowRef = useRef<HTMLLIElement | null>(null);

	const startEdit = () => {
		setDraft(entry.text);
		setProblem(null);
		setEditing(true);
	};
	const cancel = () => {
		setEditing(false);
		setProblem(null);
		requestAnimationFrame(() => rowRef.current?.querySelector<HTMLElement>("[data-edit]")?.focus());
	};
	const save = async () => {
		const text = draft.trim();
		if (!text || busy) return;
		if (text === entry.text) {
			setEditing(false);
			return;
		}
		setBusy(true);
		const outcome = await onSave(text);
		setBusy(false);
		const message = outcomeMessage(t, outcome);
		if (message) setProblem(message);
		else if (outcome?.ok) setEditing(false);
	};
	const remove = async () => {
		if (!onArm()) return;
		setBusy(true);
		const outcome = await onDelete();
		setBusy(false);
		const message = outcomeMessage(t, outcome);
		if (message) setProblem(message);
	};

	const Icon = kind === "note" ? Pin : Lightbulb;

	if (editing) {
		return (
			<li className="flex gap-2.5 rounded-lg px-2 py-2" ref={rowRef}>
				<Icon aria-hidden="true" className="mt-2.5 shrink-0 text-(--omp-dim)" size={13} />
				<div className="flex min-w-0 flex-1 flex-col gap-1.5">
					<TextArea
						aria-label={t(kind === "note" ? "memory.notes.editLabel" : "memory.lessons.editLabel")}
						autoFocus
						autoGrow
						data-owns-escape=""
						disabled={busy}
						maxLength={1000}
						onChange={event => setDraft(event.target.value)}
						onFocus={event => event.currentTarget.setSelectionRange(draft.length, draft.length)}
						onKeyDown={editorKeys(() => void save(), cancel)}
						rows={1}
						value={draft}
					/>
					<div className="flex items-center gap-2">
						<Button
							disabled={!draft.trim()}
							icon={<Check size={12} />}
							loading={busy}
							onClick={() => void save()}
							size="sm"
							variant="primary"
						>
							{t("common.save")}
						</Button>
						<Button icon={<X size={12} />} onClick={cancel} size="sm" variant="ghost">
							{t("common.cancel")}
						</Button>
						<span className="ml-auto text-omp-xs text-(--omp-dim)">{problem ?? t("memory.notes.editHint")}</span>
					</div>
				</div>
			</li>
		);
	}

	return (
		<li
			className={cx(
				"group relative flex gap-2.5 rounded-lg px-2 py-1.5 transition-colors hover:bg-(--omp-sidebar-item-hover)",
				busy && "opacity-60",
			)}
			ref={rowRef}
		>
			<Icon
				aria-hidden="true"
				className={cx("mt-[0.3rem] shrink-0", kind === "note" ? "text-(--omp-accent)" : "text-(--omp-warning)")}
				size={13}
			/>
			<div className={cx("min-w-0 flex-1", dimmed && "opacity-55")}>
				<p className="text-omp-md leading-relaxed break-words whitespace-pre-wrap text-(--omp-text)">
					{entry.text}
					{marker}
				</p>
				{entry.context && <p className="mt-0.5 text-omp-xs text-(--omp-dim)">{entry.context}</p>}
				{problem && <p className="mt-0.5 text-omp-xs text-(--omp-warning)">{problem}</p>}
			</div>
			<div
				className={cx(
					"flex shrink-0 items-start gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100",
					armed && "opacity-100",
				)}
			>
				{extraActions.map(action => (
					<IconAction icon={action.icon} key={action.id} label={action.label} onClick={action.onClick} />
				))}
				<IconAction data-edit icon={<Pencil size={13} />} label={t("common.edit")} onClick={startEdit} />
				{armed ? (
					<button
						className="h-6 rounded-md px-2 text-omp-xs font-medium text-(--omp-error) transition-colors hover:bg-(--omp-error-dim)"
						onClick={() => void remove()}
						type="button"
					>
						{t("memory.entry.confirmDelete")}
					</button>
				) : (
					<IconAction icon={<Trash2 size={13} />} label={t("common.delete")} onClick={() => void remove()} />
				)}
			</div>
		</li>
	);
}

function IconAction({
	icon,
	label,
	onClick,
	...rest
}: {
	icon: ReactNode;
	label: string;
	onClick: () => void;
	"data-edit"?: boolean;
}) {
	return (
		<button
			aria-label={label}
			className="grid size-6 place-items-center rounded-md text-(--omp-muted) transition-colors hover:bg-(--omp-bg-tertiary) hover:text-(--omp-text)"
			data-edit={rest["data-edit"] ? "" : undefined}
			onClick={onClick}
			title={label}
			type="button"
		>
			{icon}
		</button>
	);
}
