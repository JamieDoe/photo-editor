/**
 * The third-party notices (ADR 0087), as scripts/third-party-notices.mjs writes them
 * beside the web assets when the app is built.
 */
export interface NoticeEntry {
  name: string;
  version: string;
  /** The licence as the library declares it (an SPDX expression, usually). */
  licence: string;
  /** Where its source is published. */
  source: string;
  /** Ids into {@link Notices.texts}: its licence and copyright texts. */
  texts: string[];
  /** What the licence asks the app to tell people (LibRaw's LGPL). */
  note?: string;
}

export interface NoticeGroup {
  title: string;
  entries: NoticeEntry[];
}

export interface Notices {
  groups: NoticeGroup[];
  texts: Record<string, string>;
}

export const NOTICES_URL = "/third-party-notices.json";

/** The notices, or null when this build has none (a development build). */
export async function loadNotices(): Promise<Notices | null> {
  const response = await fetch(NOTICES_URL).catch(() => null);
  if (!response?.ok) return null;
  const notices = (await response.json().catch(() => null)) as Notices | null;
  return Array.isArray(notices?.groups) && typeof notices.texts === "object" ? notices : null;
}

/** The groups with only the entries whose name or licence contains `query`. */
export function filterNotices(groups: NoticeGroup[], query: string): NoticeGroup[] {
  const q = query.trim().toLowerCase();
  if (!q) return groups;
  return groups
    .map((g) => ({
      ...g,
      entries: g.entries.filter((e) => e.name.toLowerCase().includes(q) || e.licence.toLowerCase().includes(q)),
    }))
    .filter((g) => g.entries.length > 0);
}
