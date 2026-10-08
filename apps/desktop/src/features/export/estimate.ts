/** The export dialog's size estimate (ADR 0068), as the design writes it: "≈ 4.2 MB",
 *  whole megabytes from 10 up, kilobytes under one, "each" for several photos. */
export function formatEstimate(bytes: number, count: number): string {
  const mb = bytes / 1_000_000;
  const size = mb >= 10 ? `${Math.round(mb)} MB` : mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1000))} KB`;
  return `≈ ${size}${count > 1 ? " each" : ""}`;
}
