const ENGLISH_LOCALE = "en-US";

export function formatEnglishDateTime(value: Date | number | string) {
  return new Intl.DateTimeFormat(ENGLISH_LOCALE, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(value));
}

export function formatEnglishMonthDay(value: Date | number | string) {
  return new Intl.DateTimeFormat(ENGLISH_LOCALE, {
    month: "short",
    day: "numeric",
  }).format(new Date(value));
}

export function formatEnglishMonthName(value: Date | number | string) {
  return new Intl.DateTimeFormat(ENGLISH_LOCALE, {
    month: "long",
  }).format(new Date(value));
}

export function formatRelativeTime(date: Date): string {
  const diffSec = Math.floor((Date.now() - date.getTime()) / 1000);
  if (diffSec < 60) return "just now";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin} min ago`;
  const hours = Math.floor(diffMin / 60);
  const mins = diffMin % 60;
  return mins === 0 ? `${hours}h ago` : `${hours}h ${mins}m ago`;
}
