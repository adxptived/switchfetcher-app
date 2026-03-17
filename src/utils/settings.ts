import type { AppSettings } from "../types";

export const REFRESH_INTERVAL_OPTIONS = [60, 90, 120, 300] as const satisfies ReadonlyArray<
  AppSettings["base_refresh_interval_seconds"]
>;

export function validateRefreshInterval(value: number): string | null {
  if (REFRESH_INTERVAL_OPTIONS.includes(value as AppSettings["base_refresh_interval_seconds"])) {
    return null;
  }
  return "Allowed values: 60, 90, 120, 300 seconds.";
}
