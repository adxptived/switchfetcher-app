import { useEffect, useState } from "react";
import { formatRelativeTime } from "../utils/date";

interface FooterProps {
  usageLastUpdated: Date | null;
  isLoading?: boolean;
  appVersion?: string;
}

function ClockIcon() {
  return (
    <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <circle cx="12" cy="12" r="8.5" strokeWidth={1.8} />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M12 7.5v5l3 2" />
    </svg>
  );
}

export function Footer({ usageLastUpdated, isLoading = false, appVersion }: FooterProps) {
  const [, setTick] = useState(0);

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      setTick((value) => value + 1);
    }, 30000);

    return () => window.clearInterval(intervalId);
  }, []);

  const statusLabel = isLoading
    ? "Loading..."
    : usageLastUpdated
      ? `Last updated ${formatRelativeTime(usageLastUpdated)}`
      : "Never updated";

  return (
    <footer
      className="border-t px-4 py-2 text-[11px]"
      style={{ background: "var(--color-bg-header)", borderColor: "var(--color-border)" }}
    >
      <div
        className="mx-auto flex max-w-6xl items-center justify-between gap-2"
        style={{ color: "var(--color-text-muted)" }}
      >
        <div className="flex items-center gap-2">
          <ClockIcon />
          <span>{statusLabel}</span>
        </div>
        {appVersion ? <span>{`v${appVersion}`}</span> : null}
      </div>
    </footer>
  );
}
