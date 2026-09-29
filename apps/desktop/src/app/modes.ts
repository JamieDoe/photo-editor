/** Primary application modes (PRODUCT.md §7.1). Library and Edit are the workspace
 * switch; Settings opens from the top bar. Export is an action until batch export
 * exists. */
export type Mode = "library" | "edit" | "settings";

export const WORKSPACES: ReadonlyArray<{ id: Mode; label: string }> = [
  { id: "library", label: "Library" },
  { id: "edit", label: "Edit" },
];
