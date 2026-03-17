import type { Provider } from "../types";

type BadgeVariant =
  | "provider"
  | "status"
  | "plan"
  | "experimental"
  | "repair";

interface BadgeProps {
  variant: BadgeVariant;
  label: string;
  provider?: Provider;
  status?: "healthy" | "warning" | "critical" | "depleted";
  title?: string;
}

export function Badge({ variant, label, provider, status, title }: BadgeProps) {
  const classes =
    variant === "provider"
      ? provider === "codex"
        ? "border-emerald-200 bg-emerald-50 text-emerald-700"
        : provider === "claude"
          ? "border-orange-200 bg-orange-50 text-orange-700"
          : "border-blue-200 bg-blue-50 text-blue-700"
      : variant === "status"
        ? status === "healthy"
          ? "border-emerald-200 bg-emerald-50 text-emerald-700"
          : status === "warning"
            ? "border-amber-200 bg-amber-50 text-amber-700"
            : status === "critical"
              ? "border-red-200 bg-red-50 text-red-600"
              : "border-slate-200 bg-slate-100 text-slate-600"
        : variant === "repair"
          ? "border-red-200 bg-red-50 text-red-600"
          : variant === "experimental"
            ? "border-blue-200 bg-blue-50 text-blue-700"
            : "border-[color:var(--color-border)] bg-[var(--color-bg-muted)] text-[var(--color-text-secondary)]";

  return (
    <span
      title={title}
      className={`inline-flex items-center rounded-full border px-2.5 py-1 text-[11px] font-medium ${classes}`}
    >
      {label}
    </span>
  );
}
