/**
 * Toggle switch component: full-width switch with label and optional description.
 * Used primarily in settings pages for boolean configuration options.
 */

export function Toggle({
	checked,
	onChange,
	label,
	description,
	disabled,
	badge,
}: {
	checked: boolean;
	onChange: (value: boolean) => void;
	label: string;
	description?: string;
	disabled?: boolean;
	badge?: React.ReactNode;
}) {
	return (
		<button
			aria-checked={checked}
			aria-label={label}
			className="flex w-full cursor-pointer items-start justify-between gap-4 rounded-lg px-3 py-2.5 text-left transition-colors hover:bg-(--omp-sidebar-item-hover) disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent"
			disabled={disabled}
			onClick={() => onChange(!checked)}
			role="switch"
			type="button"
		>
			<span className="min-w-0">
				<span className="flex items-center gap-2">
					<span className="block text-omp-md font-medium text-(--omp-text)">{label}</span>
					{badge}
				</span>
				{description && (
					<span className="mt-0.5 block text-omp-sm leading-snug text-(--omp-muted)">{description}</span>
				)}
			</span>
			<span
				aria-hidden
				className={`omp-switch-track relative mt-0.5 h-5 w-9 shrink-0 rounded-full ${
					checked ? "bg-(--omp-switch-on)" : "bg-(--omp-switch-off)" // surface-ok: toggle switch track fill
				}`}
			>
				<span
					className={`omp-switch-thumb absolute top-0.5 left-0.5 size-4 rounded-full bg-white shadow ${
						checked ? "translate-x-4" : "translate-x-0"
					}`}
				/>
			</span>
		</button>
	);
}
