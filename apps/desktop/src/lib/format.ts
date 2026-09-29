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

/** Shutter speed as photographers write it: "1/250 s", "0.5 s", "4 s". */
export function formatShutter(seconds: number | null | undefined): string | null {
  if (!seconds || seconds <= 0) return null;
  if (seconds >= 1) return `${Number.isInteger(seconds) ? seconds : seconds.toFixed(1)} s`;
  const denominator = Math.round(1 / seconds);
  // Exposures like 0.4 s read better as decimals than as 1/2.5.
  return Math.abs(1 / denominator - seconds) / seconds < 0.05 ? `1/${denominator} s` : `${seconds.toFixed(1)} s`;
}

export function formatAperture(f: number | null | undefined): string | null {
  return f && f > 0 ? `f/${Number.isInteger(f) ? f : f.toFixed(1)}` : null;
}

export function formatFocal(mm: number | null | undefined): string | null {
  return mm && mm > 0 ? `${Math.round(mm)} mm` : null;
}

/** "ISO 100 · 52 mm · f/6.7 · 1 s", skipping anything unknown. */
export function formatExposure(e: {
  iso?: number | null;
  focalLengthMm?: number | null;
  aperture?: number | null;
  shutterSeconds?: number | null;
}): string {
  const iso = e.iso ? `ISO ${Math.round(e.iso)}` : null;
  return [iso, formatFocal(e.focalLengthMm), formatAperture(e.aperture), formatShutter(e.shutterSeconds)]
    .filter((x): x is string => x !== null)
    .join(" · ");
}

/** A camera wall-clock time ("2026-09-24T06:41:12", no zone) shown as recorded. */
export function formatCaptured(iso: string | null | undefined, locale?: string): string | null {
  if (!iso) return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(iso);
  if (!m) return null;
  // Construct as UTC and format as UTC so the wall-clock digits never shift.
  const d = new Date(Date.UTC(+m[1]!, +m[2]! - 1, +m[3]!, +m[4]!, +m[5]!));
  return d.toLocaleString(locale, { dateStyle: "medium", timeStyle: "short", timeZone: "UTC" });
}

/** Capture-date span of a set of photos ("24–26 Sep 2026"), from camera wall-clock
 *  times; null when none has one. */
export function formatDateRange(times: ReadonlyArray<string | null | undefined>, locale?: string): string | null {
  let min: number | null = null;
  let max: number | null = null;
  for (const t of times) {
    const m = t ? /^(\d{4})-(\d{2})-(\d{2})/.exec(t) : null;
    if (!m) continue;
    const day = Date.UTC(+m[1]!, +m[2]! - 1, +m[3]!);
    min = min === null ? day : Math.min(min, day);
    max = max === null ? day : Math.max(max, day);
  }
  if (min === null || max === null) return null;
  const f = new Intl.DateTimeFormat(locale, { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });
  return min === max ? f.format(min) : f.formatRange(min, max);
}
