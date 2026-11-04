import { BookMarked, Bookmark, GraduationCap, Layers, Microchip, NotebookPen, Rewind, ShieldCheck } from "lucide-react";
import type { ComponentType, ReactNode } from "react";
import { cx } from "../../lib/format";
import { useT } from "../../lib/i18n";
import { resultBodyText, resultDetails } from "./result";
import type { ToolRendererProps } from "./ToolCard";

/**
 * Dedicated cards for the tools that still fell back to GenericRenderer:
 * checkpoint, rewind, context_notes, new_context, learn, manage_skill,
 * security_scan, ida. Each surfaces its operation line plus the meaningful
 * payload (goal, report, lesson, stored note, skill fields) instead of a raw
 * args/result dump.
 */

interface OpCardProps {
	icon: ComponentType<{ size?: number; className?: string; style?: Record<string, string> }>;
	title: string;
	subtitle?: string;
	tone?: "accent" | "success" | "warning" | "muted" | "error";
	isError?: boolean;
	children?: ReactNode;
	/** Long body text rendered in the capped mono block. */
	body?: string;
}

const TONE_ICON: Record<NonNullable<OpCardProps["tone"]>, string> = {
	accent: "text-[var(--omp-accent)]",
	success: "text-[var(--omp-success)]",
	warning: "text-[var(--omp-warning)]",
	muted: "text-[var(--omp-muted)]",
	error: "text-[var(--omp-error)]",
};

function OpCard({ icon: Icon, title, subtitle, tone = "accent", isError, children, body }: OpCardProps) {
	return (
		<div className="flex flex-col gap-1.5">
			<div className="flex items-center gap-1.5 text-omp-sm">
				<Icon size={12} className={cx("shrink-0", isError ? TONE_ICON.error : TONE_ICON[tone])} />
				<span className="font-semibold text-[var(--omp-text)]">{title}</span>
				{subtitle && <span className="truncate text-[var(--omp-muted)]">{subtitle}</span>}
			</div>
			{children}
			{body && (
				<pre
					className={cx(
						"max-h-40 overflow-auto whitespace-pre-wrap rounded px-2 py-1.5 font-mono text-omp-sm leading-[1.45]",
						isError
							? "bg-[var(--omp-tool-error-bg)] text-[var(--omp-error)]"
							: "bg-[var(--omp-code-bg)] text-[var(--omp-tool-output)]",
					)}
				>
					{body}
				</pre>
			)}
		</div>
	);
}

function text(value: unknown): string {
	return typeof value === "string" ? value.trim() : "";
}

/** checkpoint {goal}: pins an investigation waypoint; the body restates the goal. */
export function CheckpointRenderer({ args, result, isError, isPartial, partialResult }: ToolRendererProps) {
	const t = useT();
	const effective = isPartial ? partialResult : result;
	const details = resultDetails(effective);
	const goal = text(details?.goal) || text(args.goal) || resultBodyText(effective).trim();
	return <OpCard icon={Bookmark} title={t("tools.checkpoint.title")} isError={isError} body={goal} />;
}

/** rewind {report}: returns control to a checkpoint with findings attached. */
export function RewindRenderer({ args, result, isError, isPartial, partialResult, interrupted }: ToolRendererProps) {
	const t = useT();
	const effective = isPartial ? partialResult : result;
	const details = resultDetails(effective);
	const report = text(details?.report) || text(args.report) || resultBodyText(effective).trim();
	return (
		<OpCard
			icon={Rewind}
			title={t("tools.rewind.title")}
			subtitle={
				details?.rewound === true ? t("tools.rewind.rewound") : interrupted ? t("tools.rewind.aborted") : undefined
			}
			isError={isError}
			body={report}
		/>
	);
}

/** context_notes {text?}: write stores the note; read returns the current one. */
export function ContextNotesRenderer({ args, result, isError, isPartial, partialResult }: ToolRendererProps) {
	const t = useT();
	const effective = isPartial ? partialResult : result;
	const details = resultDetails(effective);
	const written = text(details?.text) || text(args.text);
	const body = written || resultBodyText(effective).trim();
	return (
		<OpCard
			icon={NotebookPen}
			title={t("tools.contextNotes.title")}
			subtitle={written ? t("tools.contextNotes.saved") : undefined}
			isError={isError}
			body={body}
		/>
	);
}

/** new_context {}: the request itself is the whole call. */
export function NewContextRenderer({ result, isError, isPartial, partialResult }: ToolRendererProps) {
	const t = useT();
	const effective = isPartial ? partialResult : result;
	const details = resultDetails(effective);
	const body = resultBodyText(effective).trim();
	return (
		<OpCard
			icon={Layers}
			title={t("tools.newContext.title")}
			subtitle={details?.requested === true ? t("tools.newContext.requested") : undefined}
			isError={isError}
			body={body}
		/>
	);
}

/** learn {memory, context?, scope?, skill?}: the durable lesson plus an optional skill chip. */
export function LearnRenderer({ args, result, isError, isPartial, partialResult }: ToolRendererProps) {
	const t = useT();
	const effective = isPartial ? partialResult : result;
	const memory = text(args.memory);
	const context = text(args.context);
	const scope = text(args.scope);
	const skill = typeof args.skill === "object" && args.skill !== null ? (args.skill as Record<string, unknown>) : null;
	const skillLine = skill
		? [text(skill.action) || t("tools.learn.skill"), text(skill.name)].filter(Boolean).join(" ")
		: "";
	const body = [memory, context ? `${t("tools.learn.context")} ${context}` : "", resultBodyText(effective).trim()]
		.filter(Boolean)
		.join("\n");
	return (
		<OpCard
			icon={GraduationCap}
			title={t("tools.learn.title")}
			subtitle={[scope === "global" ? t("tools.learn.scopeGlobal") : "", skillLine].filter(Boolean).join(" · ")}
			isError={isError}
			body={body}
		/>
	);
}

/** manage_skill {action, name, description?, body?}: CRUD on a managed skill. */
export function ManageSkillRenderer({ args, result, isError, isPartial, partialResult }: ToolRendererProps) {
	const t = useT();
	const effective = isPartial ? partialResult : result;
	const details = resultDetails(effective);
	const action = text(details?.action) || text(args.action);
	const name = text(details?.name) || text(args.name);
	const description = text(details?.description) || text(args.description);
	// Past tense only once the tool actually returned — "Creating" while pending.
	const pending = effective == null && !isError;
	const actionKey = pending
		? action === "create"
			? "tools.manageSkill.creating"
			: action === "delete"
				? "tools.manageSkill.deleting"
				: "tools.manageSkill.updating"
		: action === "create"
			? "tools.manageSkill.created"
			: action === "delete"
				? "tools.manageSkill.deleted"
				: "tools.manageSkill.updated";
	const subtitle = name ? `${t(actionKey)} ${name}` : t(actionKey);
	const body = [description, resultBodyText(effective).trim()].filter(Boolean).join("\n");
	return (
		<OpCard
			icon={BookMarked}
			title={t("tools.manageSkill.title")}
			subtitle={subtitle}
			isError={isError}
			body={body}
		/>
	);
}

/** security_scan {action, ...}: local scans plus Codex cloud operations. */
export function SecurityScanRenderer({ args, result, isError, isPartial, partialResult }: ToolRendererProps) {
	const t = useT();
	const effective = isPartial ? partialResult : result;
	const details = resultDetails(effective);
	const action = text(details?.action) || text(args.action);
	const operation = details?.operation as Record<string, unknown> | undefined;
	const status = text(operation?.status) || text(operation?.phase) || (details?.cancelled === true ? "cancelled" : "");
	const subtitle = [action, status].filter(Boolean).join(" · ");
	// Cross-system references are how the scan is revisited later (Codex cloud
	// ids, the local operation id) — they stay visible instead of sinking into
	// a JSON dump.
	const refs = (["plan_id", "scan_id", "finding_id", "operation_id"] as const)
		.map(key => ({ key, value: text(args[key]) || text(details?.[key]) }))
		.filter(ref => ref.value)
		.map(ref => `${ref.key} ${ref.value}`);
	const plan = details?.plan as Record<string, unknown> | undefined;
	const planRef = text(plan?.id) ? `plan_id ${text(plan?.id)}` : "";
	if (planRef && !refs.some(ref => ref === planRef)) refs.push(planRef);
	const body = [refs.join(" · "), resultBodyText(effective).trim()].filter(Boolean).join("\n");
	return (
		<OpCard
			icon={ShieldCheck}
			title={t("tools.securityScan.title")}
			subtitle={subtitle}
			isError={isError}
			body={body}
		/>
	);
}

/** ida {action, db?, target?, ...}: IDA Pro database operations shared across agents. */
export function IdaRenderer({ args, result, isError, isPartial, partialResult }: ToolRendererProps) {
	const t = useT();
	const effective = isPartial ? partialResult : result;
	const details = resultDetails(effective);
	const action = text(details?.action) || text(args.action);
	const target = text(args.target) || text(args.db) || text(args.name);
	const body = resultBodyText(effective).trim();
	return (
		<OpCard
			icon={Microchip}
			title={t("tools.ida.title")}
			subtitle={[action, target].filter(Boolean).join(" ")}
			isError={isError}
			body={body}
		/>
	);
}
