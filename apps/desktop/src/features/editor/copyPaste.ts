import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { SettingGroup } from "../../ipc/generated/SettingGroup";

/**
 * Copying and pasting edits (ADR 0048). The engine groups the recipe's settings by
 * panel section; an edit is copied a group at a time, and pasting sets exactly those
 * settings on the target, leaving the rest as they were.
 */

/** Copied settings: the source recipe and the groups taken from it. */
export interface CopiedEdits {
  recipe: EditRecipe;
  groups: string[];
}

/** The groups copied unless the photographer chooses otherwise. */
export function defaultCopyGroups(groups: readonly SettingGroup[]): string[] {
  return groups.filter((g) => g.copiedByDefault).map((g) => g.id);
}

/** `target` with the settings of `copied`'s groups taken from its recipe: a field the
 *  source does not have (no crop, no masks) is removed from the target too. */
export function pasteEdits(target: EditRecipe, copied: CopiedEdits, groups: readonly SettingGroup[]): EditRecipe {
  const next: Record<string, unknown> = { ...target };
  const source = copied.recipe as unknown as Record<string, unknown>;
  for (const g of groups) {
    if (!copied.groups.includes(g.id)) continue;
    for (const field of g.fields) {
      if (source[field] === undefined) delete next[field];
      else next[field] = source[field];
    }
  }
  return next as unknown as EditRecipe;
}

/** Whether pasting would change `target`. */
export function pasteChanges(target: EditRecipe, copied: CopiedEdits, groups: readonly SettingGroup[]): boolean {
  const next = pasteEdits(target, copied, groups);
  return JSON.stringify(next) !== JSON.stringify(target);
}
