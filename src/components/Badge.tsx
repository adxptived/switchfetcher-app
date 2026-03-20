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
  const tone =
    variant === "provider"
      ? provider === "codex"
        ? "var(--color-codex)"
        : provider === "claude"
          ? "var(--color-claude)"
          : "var(--color-gemini)"
      : variant === "status"
        ? status === "healthy"
          ? "var(--color-codex)"
          : status === "warning"
            ? "#d97706"
            : status === "critical"
              ? "#dc2626"
              : "var(--color-text-secondary)"
        : variant === "repair"
          ? "#dc2626"
          : variant === "experimental"
            ? "var(--color-gemini)"
            : null;

  const style = tone
    ? {
        borderColor: `color-mix(in srgb, ${tone} 25%, transparent)`,
        background: `color-mix(in srgb, ${tone} 12%, transparent)`,
        color: tone,
      }
    : {
        borderColor: "var(--color-border)",
        background: "var(--color-bg-muted)",
        color: "var(--color-text-secondary)",
      };

  const className =
    variant === "provider"
      ? "inline-flex items-center rounded-md border px-2.5 py-1 text-xs font-medium"
      : "inline-flex items-center rounded-full border px-2.5 py-1 text-[11px] font-medium";

  return (
    <span
      title={title}
      className={className}
      style={style}
    >
      {label}
    </span>
  );
}
