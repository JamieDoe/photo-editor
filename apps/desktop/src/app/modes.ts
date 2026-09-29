/** Primary application modes (PRODUCT.md §7.1). Export is an action within Edit until
 * batch export exists. */
export type Mode = "library" | "edit" | "settings";

export const MODES: ReadonlyArray<{ id: Mode; label: string }> = [
  { id: "library", label: "Library" },
  { id: "edit", label: "Edit" },
  { id: "settings", label: "Settings" },
];
