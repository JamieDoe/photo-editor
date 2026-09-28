import { useCallback, useEffect, useRef, useState } from "react";
import { toAppError, type AppError } from "../../app/errors";
import * as ipc from "../../ipc/client";
import type { Settings } from "../../ipc/generated/Settings";
import type { SettingsViewDto } from "../../ipc/generated/SettingsViewDto";
import type { Theme } from "../../ipc/generated/Theme";

/** Applies the theme by setting `data-theme` on <html>; "system" follows the OS. */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
}

/** Idle time before a change is written to disk, so slider drags save once. */
const SAVE_DELAY_MS = 250;

/**
 * Settings owned by Rust (validated and persisted there). Changes show immediately;
 * saving is debounced, and Rust's stored (clamped) values are adopted when a save
 * returns, unless newer local changes are already pending.
 */
export function useSettings() {
  const [view, setView] = useState<SettingsViewDto | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const viewRef = useRef<SettingsViewDto | null>(null);
  const timerRef = useRef<number | null>(null);
  const editsRef = useRef(0); // local edits made so far

  const adopt = useCallback((v: SettingsViewDto | null) => {
    viewRef.current = v;
    setView(v);
  }, []);

  const save = useCallback(async () => {
    timerRef.current = null;
    const current = viewRef.current;
    if (!current) return;
    const editsAtSave = editsRef.current;
    try {
      const stored = await ipc.updateSettings(current.settings);
      // Ignore the response if the user changed something while it was saving.
      if (editsRef.current === editsAtSave) adopt(stored);
    } catch (e) {
      setError(await toAppError(e));
      adopt(await ipc.getSettings().catch(() => null)); // back to what is stored
    }
  }, [adopt]);

  useEffect(() => {
    ipc.getSettings().then(adopt, (e: unknown) => void toAppError(e).then(setError));
    return () => {
      // Flush a pending save when the settings consumer unmounts.
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        void save();
      }
    };
  }, [adopt, save]);

  useEffect(() => {
    if (view) applyTheme(view.settings.general.theme);
  }, [view]);

  const update = useCallback(
    (change: (s: Settings) => Settings) => {
      const current = viewRef.current;
      if (!current) return;
      editsRef.current++;
      adopt({ ...current, settings: change(current.settings) });
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => void save(), SAVE_DELAY_MS);
    },
    [adopt, save],
  );

  /** Re-reads settings changed by Rust (e.g. recent folders after choosing one). */
  const reload = useCallback(async () => {
    adopt(await ipc.getSettings().catch(() => viewRef.current));
  }, [adopt]);

  return { view, settings: view?.settings ?? null, update, adopt, reload, error, clearError: () => setError(null) };
}

export type SettingsApi = ReturnType<typeof useSettings>;
