import type { FlagRuleDto } from "../../ipc/generated/FlagRuleDto";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import type { SmartRulesDto } from "../../ipc/generated/SmartRulesDto";
import { LABELS } from "./marks";

/**
 * Smart collections (ADR 0084): named rules, all of which a photo must match. The
 * catalogue finds the photos; this mirrors the rules on marks so a photo marked out of
 * a smart collection leaves the view at once, as it does from Picks.
 */

export const NO_RULES: SmartRulesDto = { rating: null, flag: null, label: null, edited: null, words: "" };

/** The rating rule's choices: any, unrated, or at least 1–5 stars. */
export const RATING_RULES: ReadonlyArray<{ value: number | null; label: string }> = [
  { value: null, label: "Any rating" },
  { value: 0, label: "Unrated" },
  { value: 1, label: "★ 1 and up" },
  { value: 2, label: "★ 2 and up" },
  { value: 3, label: "★ 3 and up" },
  { value: 4, label: "★ 4 and up" },
  { value: 5, label: "★ 5" },
];

export const FLAG_RULES: ReadonlyArray<{ value: FlagRuleDto | null; label: string }> = [
  { value: null, label: "Any flag" },
  { value: "picked", label: "Picked" },
  { value: "notRejected", label: "Not rejected" },
  { value: "unflagged", label: "Unflagged" },
  { value: "rejected", label: "Rejected" },
];

export const EDITED_RULES: ReadonlyArray<{ value: boolean | null; label: string }> = [
  { value: null, label: "Edited or not" },
  { value: true, label: "Edited" },
  { value: false, label: "Not edited" },
];

/** No rules: every photo would match (that is All photos, not a smart collection). */
export function hasNoRules(r: SmartRulesDto): boolean {
  return r.rating === null && r.flag === null && r.label === null && r.edited === null && r.words.trim() === "";
}

/** Whether `photo`'s marks and edit match `rules` (words are the catalogue's to judge). */
export function matchesMarks(photo: PhotoEntryDto, rules: SmartRulesDto): boolean {
  const { rating, flag, label } = photo.marks;
  if (rules.rating !== null && (rules.rating === 0 ? rating !== 0 : rating < rules.rating)) return false;
  switch (rules.flag) {
    case "picked":
      if (flag !== "pick") return false;
      break;
    case "rejected":
      if (flag !== "reject") return false;
      break;
    case "unflagged":
      if (flag !== "none") return false;
      break;
    case "notRejected":
      if (flag === "reject") return false;
      break;
    case null:
      break;
  }
  if (rules.label !== null && label !== rules.label) return false;
  if (rules.edited !== null && photo.edited !== rules.edited) return false;
  return true;
}

/** The rules in a few words, as the header shows them: "★ 3 and up · Picked · Red · “iceland”". */
export function rulesSummary(r: SmartRulesDto): string {
  const parts: string[] = [];
  if (r.rating !== null) parts.push(RATING_RULES.find((x) => x.value === r.rating)?.label ?? "");
  if (r.flag !== null) parts.push(FLAG_RULES.find((x) => x.value === r.flag)?.label ?? "");
  if (r.label !== null) parts.push(r.label === "none" ? "No label" : (LABELS.find((l) => l.id === r.label)?.name ?? ""));
  if (r.edited !== null) parts.push(r.edited ? "Edited" : "Not edited");
  if (r.words.trim()) parts.push(`“${r.words.trim().replace(/\s+/g, " ")}”`);
  return parts.filter(Boolean).join(" · ");
}
