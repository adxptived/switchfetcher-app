import type { AccountWithUsage, Provider } from "../types";

export function getRemainingPercent(account: AccountWithUsage) {
  if (account.usage?.primary_used_percent == null) return -1;
  return Math.max(0, 100 - account.usage.primary_used_percent);
}

export function formatPlanLabel(
  planType: string | null | undefined,
  authMode: AccountWithUsage["auth_mode"],
) {
  if (!planType) {
    return authMode === "api_key" ? "API Key" : "Unknown";
  }
  if (planType === planType.toUpperCase()) {
    return planType;
  }
  return planType.charAt(0).toUpperCase() + planType.slice(1);
}

export function matchesSearch(account: AccountWithUsage, query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;
  return [
    account.name,
    account.email ?? "",
    ...account.tags,
  ].some((value) => value.toLowerCase().includes(normalized));
}

export function computeLoadedBestAccount(accounts: AccountWithUsage[], provider: Provider) {
  return accounts
    .filter(
      (account) =>
        account.provider === provider &&
        account.load_state === "ready" &&
        account.capabilities.supports_switch &&
        !account.hidden &&
        !account.usage?.error,
    )
    .sort((left, right) => {
      const remainingDiff = getRemainingPercent(right) - getRemainingPercent(left);
      if (remainingDiff !== 0) return remainingDiff;
      const leftReset = left.usage?.primary_resets_at ?? Number.MAX_SAFE_INTEGER;
      const rightReset = right.usage?.primary_resets_at ?? Number.MAX_SAFE_INTEGER;
      if (leftReset !== rightReset) return leftReset - rightReset;
      return left.name.localeCompare(right.name);
    })[0];
}
