import { useState, useRef, useEffect } from "react";
import type { AccountWithUsage } from "../types";
import { UsageBar } from "./UsageBar";
import { Badge } from "./Badge";
import { formatPlanLabel } from "../utils/accounts";
import { formatEnglishMonthDay } from "../utils/date";

interface AccountCardProps {
  account: AccountWithUsage;
  onSwitch: () => void;
  onWarmup: () => Promise<void>;
  onDelete: () => void;
  onRefresh: () => Promise<void>;
  onRepair?: () => Promise<void> | void;
  onRename: (newName: string) => Promise<void>;
  onUpdateTags?: () => void;
  onToggleSelect?: () => void;
  selectionMode?: boolean;
  selected?: boolean;
  switching?: boolean;
  switchDisabled?: boolean;
  switchDisabledReason?: string;
  warmingUp?: boolean;
  masked?: boolean;
  onToggleMask?: () => void;
  use24hTime?: boolean;
}

function BlurredText({ children, blur }: { children: React.ReactNode; blur: boolean }) {
  return (
    <span
      className={`transition-all duration-[var(--transition-base)] select-none ${blur ? "blur-sm" : ""}`}
      style={blur ? { userSelect: "none" } : undefined}
    >
      {children}
    </span>
  );
}

function formatResetDate(value: number | null | undefined) {
  if (!value) return "No reset window";
  return formatEnglishMonthDay(value * 1000);
}

export function AccountCard({
  account,
  onSwitch,
  onWarmup,
  onDelete,
  onRefresh,
  onRepair,
  onRename,
  onUpdateTags,
  onToggleSelect,
  selectionMode = false,
  selected = false,
  switching,
  switchDisabled,
  switchDisabledReason,
  warmingUp,
  masked = false,
  onToggleMask,
  use24hTime = false,
}: AccountCardProps) {
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [editName, setEditName] = useState(account.name);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  const handleRefresh = async () => {
    setIsRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleRename = async () => {
    const trimmed = editName.trim();
    if (trimmed && trimmed !== account.name) {
      try {
        await onRename(trimmed);
      } catch {
        setEditName(account.name);
      }
    } else {
      setEditName(account.name);
    }
    setIsEditing(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleRename();
    } else if (e.key === "Escape") {
      setEditName(account.name);
      setIsEditing(false);
    }
  };

  const effectivePlanType = account.usage?.plan_type ?? account.plan_type;
  const planDisplay = formatPlanLabel(effectivePlanType, account.auth_mode);
  const providerLabel =
    account.provider.charAt(0).toUpperCase() + account.provider.slice(1);
  const showWarmup = account.capabilities.supports_warmup;
  const providerAccent: Record<typeof account.provider, string> = {
    codex: "var(--color-codex)",
    claude: "var(--color-claude)",
    gemini: "var(--color-gemini)",
  };
  const accentColor = providerAccent[account.provider];

  const needsRepair = account.load_state === "needs_repair";
  const primaryPercent = account.usage?.primary_used_percent;
  const resetLabel = formatResetDate(account.usage?.primary_resets_at);
  const usageLabel =
    primaryPercent == null ? "Usage unavailable" : `${Math.round(primaryPercent)}% used`;


  return (
    <div
      className="relative rounded-2xl border p-5 transition-all duration-[var(--transition-base)]"
      style={{
        background: "var(--color-bg-card)",
        borderColor: account.is_active ? accentColor : "var(--color-border)",
        borderLeft: `${account.is_active ? 5 : 4}px solid ${accentColor}`,
        boxShadow: account.is_active
          ? `0 0 0 1px ${accentColor}`
          : "0 10px 30px rgba(15, 23, 42, 0.04)",
      }}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          <div className="mb-2 flex flex-wrap items-center gap-2">
            <Badge variant="provider" provider={account.provider} label={providerLabel} />
            {effectivePlanType || account.auth_mode === "api_key" ? (
              <Badge variant="plan" label={planDisplay} />
            ) : null}
            {account.provider === "gemini" ? (
              <Badge
                variant="experimental"
                label="BETA"
                title="Gemini support is experimental. Usage may be unavailable."
              />
            ) : null}
            {needsRepair ? (
              <Badge
                variant="repair"
                label="Needs repair"
                title={account.unavailable_reason ?? "Account credentials need repair before normal actions resume."}
              />
            ) : null}
          </div>

          <div className="flex items-center gap-2 mb-1">
            {account.is_active && (
              <span className="flex h-2.5 w-2.5">
                <span
                  className="animate-ping absolute inline-flex h-2.5 w-2.5 rounded-full opacity-60"
                  style={{ background: accentColor }}
                ></span>
                <span
                  className="relative inline-flex h-2.5 w-2.5 rounded-full"
                  style={{ background: accentColor }}
                ></span>
              </span>
            )}
            {isEditing ? (
              <input
                ref={inputRef}
                type="text"
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                onBlur={handleRename}
                onKeyDown={handleKeyDown}
                className="w-full rounded-lg px-2 py-1 font-semibold"
                style={{
                  color: "var(--color-text-primary)",
                  background: "var(--color-bg-muted)",
                  border: "1px solid var(--color-border-hover)",
                }}
              />
            ) : (
              <h3
                className="truncate text-base font-semibold cursor-pointer"
                style={{ color: "var(--color-text-primary)" }}
                onClick={() => {
                  if (masked || needsRepair) return;
                  setEditName(account.name);
                  setIsEditing(true);
                }}
                title={masked ? undefined : needsRepair ? "Repair account before renaming" : "Click to rename"}
              >
                <BlurredText blur={masked}>{account.name}</BlurredText>
              </h3>
            )}
          </div>
          {account.email && (
            <p
              className="truncate text-sm"
              style={{ color: "var(--color-text-secondary)" }}
            >
              <BlurredText blur={masked}>{account.email}</BlurredText>
            </p>
          )}
        </div>

        <div className="flex items-center gap-2">
          {selectionMode && onToggleSelect ? (
            <button
              onClick={onToggleSelect}
              aria-label={selected ? "Unselect account" : "Select account"}
              className="rounded-md border px-2 py-1 text-xs"
              style={
                selected
                  ? {
                      background: "rgb(254 243 199)",
                      color: "rgb(146 64 14)",
                      borderColor: "rgb(252 211 77)",
                    }
                  : {
                      background: "var(--color-bg-muted)",
                      color: "var(--color-text-secondary)",
                      borderColor: "var(--color-border)",
                    }
              }
              title={selected ? "Unselect account" : "Select account"}
            >
              {selected ? "Selected" : "Select"}
            </button>
          ) : null}
          {onToggleMask && (
            <button
              onClick={onToggleMask}
              aria-label={masked ? "Show account details" : "Hide account details"}
              className="p-1 transition-colors"
              style={{ color: "var(--color-text-muted)" }}
              title={masked ? "Show info" : "Hide info"}
            >
              {masked ? (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                </svg>
              ) : (
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                </svg>
              )}
            </button>
          )}
        </div>
      </div>

      <div
        className="my-3 border-t pt-3"
        style={{ borderColor: "var(--color-border)" }}
      >
        <div className="mb-2 flex items-center justify-between gap-3">
          <div className="text-sm font-medium" style={{ color: "var(--color-text-primary)" }}>
            Usage
          </div>
          <div className="text-sm font-semibold" style={{ color: accentColor }}>
            {usageLabel}
          </div>
        </div>
        <UsageBar
          usage={account.usage}
          loading={isRefreshing || account.usageLoading}
          use24hTime={use24hTime}
        />
        {account.usage?.skipped ? (
          <div className="mt-2">
            <Badge
              variant="status"
              status="warning"
              label="Usage only for active account"
              title="Usage is only available for the active account."
            />
          </div>
        ) : null}
        <div className="mt-2 flex items-center justify-between gap-3 text-xs">
          <span style={{ color: "var(--color-text-secondary)" }}>
            Reset window
          </span>
          <span style={{ color: "var(--color-text-primary)" }}>{resetLabel}</span>
        </div>
      </div>

      <div
        className="mb-3 border-t pt-3 space-y-1"
        style={{ borderColor: "var(--color-border)" }}
      >
        {account.last_refresh_error ? (
          <div className="text-xs text-red-500">
            Last refresh error: {account.last_refresh_error.detail ?? account.last_refresh_error.summary}
          </div>
        ) : null}
        {needsRepair && account.unavailable_reason ? (
          <div className="text-xs text-red-600" title="This account could not be loaded from secure storage. Repair it from provider files or re-import it.">
            {account.unavailable_reason}
          </div>
        ) : null}
        {needsRepair && account.repair_hint ? (
          <div className="text-xs" style={{ color: "var(--color-text-secondary)" }}>
            {account.repair_hint}
          </div>
        ) : null}
        {account.tags.length > 0 ? (
          <div className="flex flex-wrap gap-1">
            {account.tags.map((tag) => (
              <span
                key={tag}
                className="inline-flex items-center rounded-full border px-2 py-0.5 text-[11px]"
                style={{
                  borderColor: "var(--color-border)",
                  background: "var(--color-bg-muted)",
                  color: "var(--color-text-secondary)",
                }}
              >
                #{tag}
              </span>
            ))}
          </div>
        ) : null}
      </div>

      <div className="flex flex-wrap gap-1.5">
        {account.is_active ? (
          <button
            disabled
            className="flex-1 rounded-lg border px-4 py-2 text-sm font-medium cursor-default"
            style={{
              background: "var(--color-bg-muted)",
              color: "var(--color-text-secondary)",
              borderColor: "var(--color-border)",
            }}
          >
            ✓ Active
          </button>
        ) : (
          <button
            onClick={onSwitch}
            disabled={switching || switchDisabled}
            className="flex-1 rounded-lg px-4 py-2 text-sm font-medium transition-colors disabled:opacity-50"
            style={
              switchDisabled
                ? {
                    background: "var(--color-bg-muted)",
                    color: "var(--color-text-muted)",
                    cursor: "not-allowed",
                  }
                : {
                    background: "var(--color-btn-primary-bg)",
                    color: "var(--color-btn-primary-text)",
                  }
            }
            title={switchDisabled ? switchDisabledReason : undefined}
          >
            {switching
              ? "Switching..."
              : switchDisabled
                ? switchDisabledReason?.includes("Gemini")
                  ? "Usage Only"
                  : switchDisabledReason?.includes("Close all Codex")
                    ? "Codex Running"
                    : "Switch Blocked"
                : "Switch"}
          </button>
        )}
        {showWarmup ? (
          <button
            onClick={() => {
              void onWarmup();
            }}
            disabled={warmingUp || needsRepair}
            className="rounded-lg px-3 py-2 text-sm transition-colors"
            style={
              warmingUp
                ? {
                    background: "var(--color-bg-muted)",
                    color: "var(--color-text-muted)",
                  }
                : {
                    background: "var(--color-btn-secondary-bg)",
                    color: "var(--color-btn-secondary-text)",
                  }
            }
            aria-label="Warm up account"
            title={warmingUp ? "Sending warm-up request..." : "Send minimal warm-up request"}
          >
            ⚡
          </button>
        ) : null}
        <button
          onClick={handleRefresh}
          disabled={isRefreshing || needsRepair}
          aria-label="Refresh usage"
          className="rounded-lg px-3 py-2 text-sm transition-colors"
          style={
            isRefreshing
              ? {
                  background: "var(--color-bg-muted)",
                  color: "var(--color-text-muted)",
                }
              : {
                  background: "var(--color-btn-secondary-bg)",
                  color: "var(--color-btn-secondary-text)",
                }
          }
          title="Refresh usage"
        >
          <span className={isRefreshing ? "animate-spin inline-block" : ""}>↻</span>
        </button>
        <button
          onClick={onDelete}
          aria-label="Delete account"
          className="rounded-lg px-3 py-2 text-sm transition-colors"
          style={{
            background: "var(--color-btn-secondary-bg)",
            color: "#dc2626",
          }}
          title="Remove account"
        >
          ✕
        </button>
        {onUpdateTags ? (
          <button
            onClick={onUpdateTags}
            disabled={needsRepair}
            aria-label="Edit account tags"
            className="rounded-lg px-3 py-2 text-sm transition-colors"
            style={{
              background: "var(--color-btn-secondary-bg)",
              color: needsRepair
                ? "var(--color-text-muted)"
                : "var(--color-btn-secondary-text)",
            }}
            title={needsRepair ? "Repair account before editing tags" : "Edit tags"}
          >
            #
          </button>
        ) : null}
        {needsRepair && onRepair ? (
          <>
            <div className="w-full text-xs" style={{ color: "var(--color-text-secondary)" }}>
              Repair will try to restore credentials from the provider files on disk.
            </div>
            <button
              onClick={() => { void onRepair(); }}
              aria-label="Repair account credentials"
              className="px-3 py-2 text-sm rounded-lg bg-red-50 hover:bg-red-100 text-red-700 transition-colors"
              title="Try to recover credentials from provider files"
            >
              Repair account
            </button>
          </>
        ) : null}
      </div>
    </div>
  );
}
