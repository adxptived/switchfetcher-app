import { useEffect, useState } from "react";
import type { ReactNode, RefObject } from "react";
import type {
  ClaudeProcessInfo,
  CodexProcessInfo,
  GeminiProcessInfo,
} from "../types";
import { formatRelativeTime } from "../utils/date";
import { Badge } from "./Badge";
import appIcon from "../assets/icon.png";

interface HeaderProps {
  theme: "light" | "dark";
  appVersion: string;
  codexProcessInfo: CodexProcessInfo | null;
  claudeProcessInfo: ClaudeProcessInfo | null;
  geminiProcessInfo: GeminiProcessInfo | null;
  hasRunningProcesses: boolean;
  isActionsMenuOpen: boolean;
  actionsMenuRef: RefObject<HTMLDivElement | null>;
  isRefreshing: boolean;
  isWarmingAllCodex: boolean;
  usageLastUpdated: Date | null;
  isUsageLoading: boolean;
  onThemeToggle: () => void;
  onOpenSettings: () => void;
  onOpenHistory: () => void;
  onOpenDiagnostics: () => void;
  onOpenRecommendationPicker: () => void;
  onRefresh: () => void;
  onWarmupAllCodex: () => void;
  onToggleActionsMenu: () => void;
  onOpenAddModal: () => void;
  onExportSlimText: () => void;
  onOpenImportSlimText: () => void;
  onExportFullFile: () => void;
  onImportFullFile: () => void;
}

function MoonIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={1.8}
        d="M21 12.8A9 9 0 1111.2 3a7 7 0 009.8 9.8z"
      />
    </svg>
  );
}

function SunIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <circle cx="12" cy="12" r="4" strokeWidth={1.8} />
      <path
        strokeLinecap="round"
        strokeWidth={1.8}
        d="M12 2.5v2.2M12 19.3v2.2M21.5 12h-2.2M4.7 12H2.5M18.7 5.3l-1.6 1.6M6.9 17.1l-1.6 1.6M18.7 18.7l-1.6-1.6M6.9 6.9L5.3 5.3"
      />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={1.8}
        d="M10.5 6h9.75M10.5 6a1.5 1.5 0 1 1-3 0m3 0a1.5 1.5 0 1 0-3 0M3.75 6H7.5m3 12h9.75m-9.75 0a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m-3.75 0H7.5m9-6h3.75m-3.75 0a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m-9.75 0h9.75"
      />
    </svg>
  );
}

function ChartIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M4 19.5h16" />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M7 16V10" />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M12 16V6" />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M17 16v-3" />
    </svg>
  );
}

function ClockIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <circle cx="12" cy="12" r="8.5" strokeWidth={1.8} />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M12 7.5v5l3 2" />
    </svg>
  );
}

function IconButton({
  title,
  onClick,
  children,
}: {
  title: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      title={title}
      className="flex h-10 w-10 items-center justify-center rounded-lg sf-btn-secondary"
      aria-label={title}
    >
      {children}
    </button>
  );
}

function HeaderStatus({
  usageLastUpdated,
  isUsageLoading,
}: {
  usageLastUpdated: Date | null;
  isUsageLoading: boolean;
}) {
  const [, setTick] = useState(0);

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      setTick((value) => value + 1);
    }, 30000);

    return () => window.clearInterval(intervalId);
  }, []);

  const label = isUsageLoading
    ? "Updating cache..."
    : usageLastUpdated
      ? `Updated ${formatRelativeTime(usageLastUpdated)}`
      : "Cache not loaded";

  return (
    <div
      className="inline-flex h-10 items-center gap-2 rounded-xl border px-3 text-sm"
      style={{
        borderColor: "var(--color-border)",
        background: "color-mix(in srgb, var(--color-bg-muted) 76%, transparent)",
        color: "var(--color-text-secondary)",
      }}
      title={usageLastUpdated ? `Usage cache timestamp: ${usageLastUpdated.toLocaleString()}` : "No cached usage timestamp yet"}
    >
      <span
        className={`flex h-2.5 w-2.5 rounded-full ${isUsageLoading ? "animate-pulse" : ""}`}
        style={{
          background: isUsageLoading
            ? "#d97706"
            : usageLastUpdated
              ? "var(--color-codex)"
              : "var(--color-text-muted)",
        }}
      />
      <ClockIcon />
      <span className="whitespace-nowrap">{label}</span>
    </div>
  );
}

export function Header({
  theme,
  appVersion,
  codexProcessInfo,
  claudeProcessInfo,
  geminiProcessInfo,
  hasRunningProcesses,
  isActionsMenuOpen,
  actionsMenuRef,
  isRefreshing,
  isWarmingAllCodex,
  usageLastUpdated,
  isUsageLoading,
  onThemeToggle,
  onOpenSettings,
  onOpenHistory,
  onOpenDiagnostics,
  onOpenRecommendationPicker,
  onRefresh,
  onWarmupAllCodex,
  onToggleActionsMenu,
  onOpenAddModal,
  onExportSlimText,
  onOpenImportSlimText,
  onExportFullFile,
  onImportFullFile,
}: HeaderProps) {
  return (
    <header
      className="sticky top-0 z-40 border-b"
      style={{ background: "var(--color-bg-header)", borderColor: "var(--color-border)" }}
    >
      <div className="mx-auto max-w-6xl px-6 py-4">
        <div className="flex flex-col gap-3">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-3">
              <img src={appIcon} alt="Switchfetcher" className="h-10 w-10 rounded-xl" />
              <div className="flex items-center gap-2">
                <h1
                  className="text-xl font-bold tracking-tight"
                  style={{ color: "var(--color-text-primary)" }}
                >
                  Switchfetcher
                </h1>
                <span className="text-xs" style={{ color: "var(--color-text-secondary)" }}>
                  {appVersion ? `v${appVersion}` : "Loading version..."}
                </span>
              </div>
            </div>
            <div className="flex flex-wrap items-center justify-end gap-2">
              {codexProcessInfo ? (
                hasRunningProcesses ? (
                  <Badge
                    variant="status"
                    status="warning"
                    label={`${codexProcessInfo.count} Codex running`}
                  />
                ) : (
                  <Badge variant="status" status="healthy" label="0 Codex" />
                )
              ) : null}
              {claudeProcessInfo ? (
                claudeProcessInfo.count > 0 ? (
                  <span
                    className="inline-flex items-center rounded-full border px-2.5 py-1 text-[11px] font-medium"
                    style={{
                      borderColor: "color-mix(in srgb, var(--color-claude) 25%, transparent)",
                      background: "color-mix(in srgb, var(--color-claude) 12%, transparent)",
                      color: "var(--color-claude)",
                    }}
                  >
                    {claudeProcessInfo.count} Claude running
                  </span>
                ) : (
                  <Badge variant="status" status="healthy" label="0 Claude" />
                )
              ) : null}
              {geminiProcessInfo?.count ? (
                <span
                  className="inline-flex items-center rounded-full border px-2.5 py-1 text-[11px] font-medium"
                  style={{
                    borderColor: "color-mix(in srgb, var(--color-gemini) 25%, transparent)",
                    background: "color-mix(in srgb, var(--color-gemini) 12%, transparent)",
                    color: "var(--color-gemini)",
                  }}
                >
                  {geminiProcessInfo.count} Gemini running
                </span>
              ) : null}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <HeaderStatus usageLastUpdated={usageLastUpdated} isUsageLoading={isUsageLoading} />
            <button
              onClick={onOpenRecommendationPicker}
              className="h-10 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-700"
            >
              Switch To Best
            </button>
            <div className="relative" ref={actionsMenuRef}>
              <button
                onClick={onToggleActionsMenu}
                className="h-10 rounded-lg px-4 py-2 text-sm font-medium sf-btn-primary"
              >
                Account ▾
              </button>
              {isActionsMenuOpen ? (
                <div className="absolute right-0 z-50 mt-2 w-64 rounded-xl p-2 sf-panel">
                  <button
                    onClick={onOpenAddModal}
                    className="w-full rounded-lg px-3 py-2 text-left text-sm sf-btn-secondary"
                  >
                    + Add Account
                  </button>
                  <button
                    onClick={onExportSlimText}
                    className="mt-1 w-full rounded-lg px-3 py-2 text-left text-sm sf-btn-secondary"
                  >
                    Export Slim Text
                  </button>
                  <button
                    onClick={onOpenImportSlimText}
                    className="mt-1 w-full rounded-lg px-3 py-2 text-left text-sm sf-btn-secondary"
                  >
                    Import Slim Text
                  </button>
                  <button
                    onClick={onExportFullFile}
                    className="mt-1 w-full rounded-lg px-3 py-2 text-left text-sm sf-btn-secondary"
                  >
                    Export Full Encrypted File
                  </button>
                  <button
                    onClick={onImportFullFile}
                    className="mt-1 w-full rounded-lg px-3 py-2 text-left text-sm sf-btn-secondary"
                  >
                    Import Full Encrypted File
                  </button>
                  <div
                    className="my-2 border-t"
                    style={{ borderColor: "var(--color-border)" }}
                  />
                  <button
                    onClick={onWarmupAllCodex}
                    disabled={isWarmingAllCodex}
                    className="w-full rounded-lg px-3 py-2 text-left text-sm sf-btn-secondary disabled:opacity-50"
                  >
                    {isWarmingAllCodex ? "Warming..." : "Warm-up All Codex"}
                  </button>
                </div>
              ) : null}
            </div>
            <button
              onClick={onRefresh}
              disabled={isRefreshing}
              title="Refresh usage data"
              className="h-10 rounded-lg px-4 py-2 text-sm font-medium sf-btn-secondary disabled:opacity-50"
            >
              {isRefreshing ? "Refreshing..." : "Refresh All"}
            </button>
            <IconButton title="Open settings" onClick={onOpenSettings}>
              <GearIcon />
            </IconButton>
            <IconButton title="Open diagnostics" onClick={onOpenDiagnostics}>
              <ChartIcon />
            </IconButton>
            <IconButton title="Open history" onClick={onOpenHistory}>
              <ClockIcon />
            </IconButton>
            <IconButton
              title={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
              onClick={onThemeToggle}
            >
              {theme === "dark" ? <SunIcon /> : <MoonIcon />}
            </IconButton>
          </div>
        </div>
      </div>
    </header>
  );
}
