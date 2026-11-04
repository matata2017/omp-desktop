import { Download, Loader2 } from "lucide-react";
import type { CSSProperties } from "react";
import type { DictationView } from "../../lib/dictation";
import { useT } from "../../lib/i18n";

/** Bar weights of the level meter: loudest in the middle. */
const METER_WEIGHTS = [0.5, 0.8, 1, 0.75, 0.45];

/**
 * The composer's dictation line: a level meter and the words being recognized while recording,
 * otherwise what dictation is waiting on. Finished words are already in the textarea above.
 */
export function DictationStrip({
	view,
	onDone,
	onCancel,
}: {
	view: DictationView;
	onDone: () => void;
	onCancel: () => void;
}) {
	const t = useT();
	const recording = view.phase === "recording";
	// Once every file is down, the wait is the model loading.
	const downloading = view.phase === "downloading" && (view.download?.percent ?? 0) < 100;
	const text =
		view.phase === "opening"
			? t("voice.dictation.opening")
			: view.phase === "preparing" || view.phase === "downloading"
				? downloading
					? t("voice.dictation.downloading", { percent: view.download?.percent ?? 0 })
					: t("voice.dictation.preparing")
				: view.phase === "transcribing"
					? view.partial || t("voice.dictation.finishing")
					: view.partial || t("voice.dictation.listening");
	const live = (recording || view.phase === "transcribing") && view.partial !== "";

	return (
		<div
			role="status"
			aria-label={t("voice.dictation.region")}
			className="omp-dictation omp-fade-in"
			data-phase={view.phase}
			data-downloading={downloading || undefined}
			title={downloading ? view.download?.label : undefined}
		>
			<span className="omp-dictation-indicator" aria-hidden="true">
				{recording ? (
					<span className="omp-dictation-meter" style={{ "--omp-dictation-level": view.level } as CSSProperties}>
						{METER_WEIGHTS.map((weight, index) => (
							<span key={index} style={{ "--omp-dictation-weight": weight } as CSSProperties} />
						))}
					</span>
				) : downloading ? (
					<Download size={14} />
				) : (
					<Loader2 size={14} className="animate-spin" />
				)}
			</span>
			<span className={live ? "omp-dictation-text omp-dictation-text--live" : "omp-dictation-text"}>
				<span dir="ltr">{text}</span>
			</span>
			<span className="flex shrink-0 items-center gap-1">
				{recording && (
					<button type="button" className="omp-dictation-action" onClick={onDone}>
						{t("voice.dictation.done")}
						<kbd>↵</kbd>
					</button>
				)}
				<button type="button" className="omp-dictation-action" onClick={onCancel}>
					{recording ? t("voice.dictation.discard") : t("voice.dictation.cancel")}
					<kbd>esc</kbd>
				</button>
			</span>
			{downloading && (
				<span className="omp-dictation-progress" aria-hidden="true">
					<span style={{ width: `${view.download?.percent ?? 0}%` }} />
				</span>
			)}
		</div>
	);
}
