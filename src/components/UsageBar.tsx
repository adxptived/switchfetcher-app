import type { UsageInfo } from "../types";

interface UsageBarProps {
  usage?: UsageInfo;
  loading?: boolean;
  use24hTime?: boolean;
}

function formatResetTime(resetAt: number | null | undefined): string {
  if (!resetAt) return "";
  const now = Math.floor(Date.now() / 1000);
  const diff = resetAt - now;
  if (diff <= 0) return "now";
  if (diff < 60) return `${diff}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m`;
  return `${Math.floor(diff / 3600)}h ${Math.floor((diff % 3600) / 60)}m`;
}

function formatExactResetTime(resetAt: number | null | undefined, use24h: boolean): string {
  if (!resetAt) return "";

  const date = new Date(resetAt * 1000);
  const month = new Intl.DateTimeFormat(undefined, { month: "long" }).format(date);
  const day = date.getDate();
  const hours = date.getHours();
  const minutes = String(date.getMinutes()).padStart(2, "0");
  if (use24h) {
    return `${month} ${day}, ${String(hours).padStart(2, "0")}:${minutes}`;
  }
  const period = date.getHours() >= 12 ? "PM" : "AM";
  const hour12 = date.getHours() % 12 || 12;

  return `${month} ${day}, ${hour12}:${minutes} ${period}`;
}

function formatWindowDuration(minutes: number | null | undefined): string {
  if (!minutes) return "";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

function RateLimitBar({
  label,
  usedPercent,
  windowMinutes,
  resetsAt,
  use24hTime = false,
}: {
  label: string;
  usedPercent: number;
  windowMinutes?: number | null;
  resetsAt?: number | null;
  use24hTime?: boolean;
}) {
  // Calculate remaining percentage
  const remainingPercent = Math.max(0, 100 - usedPercent);
  
  // Color based on remaining (green = plenty left, red = almost none left)
  const colorClass =
    remainingPercent <= 10
      ? "bg-red-500"
      : remainingPercent <= 30
        ? "bg-amber-500"
        : "bg-emerald-500";

  const windowLabel = formatWindowDuration(windowMinutes);
  const resetLabel = formatResetTime(resetsAt);
  const exactResetLabel = formatExactResetTime(resetsAt, use24hTime);

  return (
    <div className="space-y-1">
      <div
        className="flex justify-between text-xs"
        style={{ color: "var(--color-text-secondary)" }}
      >
        <span>{label} {windowLabel && `(${windowLabel})`}</span>
        <span>
          {remainingPercent.toFixed(0)}% left
          {resetLabel && ` • resets ${resetLabel}`}
          {resetLabel && exactResetLabel && ` (${exactResetLabel})`}
        </span>
      </div>
      <div
        className="h-1.5 rounded-full overflow-hidden"
        style={{ background: "var(--color-bg-muted)" }}
      >
        <div
          className={`h-full transition-all duration-[var(--transition-slow)] ${colorClass}`}
          style={{ width: `${Math.min(remainingPercent, 100)}%` }}
        ></div>
      </div>
    </div>
  );
}

export function UsageBar({ usage, loading, use24hTime = false }: UsageBarProps) {
  if (loading) {
    return (
      <div className="space-y-2">
        <div
          className="h-1.5 rounded-full overflow-hidden animate-pulse"
          style={{ background: "var(--color-bg-muted)" }}
        >
          <div
            className="h-full w-2/3"
            style={{ background: "var(--color-border)" }}
          ></div>
        </div>
        <div
          className="h-1.5 rounded-full overflow-hidden animate-pulse"
          style={{ background: "var(--color-bg-muted)" }}
        >
          <div
            className="h-full w-1/2"
            style={{ background: "var(--color-border)" }}
          ></div>
        </div>
      </div>
    );
  }

  if (!usage || usage.error) {
    return (
      <div
        className="text-xs italic py-1"
        style={{ color: "var(--color-text-muted)" }}
      >
        {usage?.error || "Usage unavailable"}
      </div>
    );
  }

  const hasPrimary = usage.primary_used_percent !== null && usage.primary_used_percent !== undefined;
  const hasSecondary = usage.secondary_used_percent !== null && usage.secondary_used_percent !== undefined;

  if (!hasPrimary && !hasSecondary) {
    return (
      <div
        className="text-xs italic py-1"
        style={{ color: "var(--color-text-muted)" }}
      >
        No rate limit data
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {hasPrimary && (
        <RateLimitBar
          label="5h Limit"
          usedPercent={usage.primary_used_percent!}
          windowMinutes={usage.primary_window_minutes}
          resetsAt={usage.primary_resets_at}
          use24hTime={use24hTime}
        />
      )}
      {hasSecondary && (
        <RateLimitBar
          label="Weekly Limit"
          usedPercent={usage.secondary_used_percent!}
          windowMinutes={usage.secondary_window_minutes}
          resetsAt={usage.secondary_resets_at}
          use24hTime={use24hTime}
        />
      )}
      {usage.credits_balance && (
        <div className="text-xs" style={{ color: "var(--color-text-secondary)" }}>
          Credits: {usage.credits_balance}
        </div>
      )}
    </div>
  );
}
