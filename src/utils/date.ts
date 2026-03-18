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
