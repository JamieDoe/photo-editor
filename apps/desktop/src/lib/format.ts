/** Human-readable file size (decimal units, as file managers show them). */
export function formatBytes(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1000;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** Local date and time for a Unix timestamp in milliseconds ("—" if unknown). */
export function formatDateTime(ms: number, locale?: string): string {
  if (!ms) return "—";
  return new Date(ms).toLocaleString(locale, { dateStyle: "medium", timeStyle: "short" });
}
