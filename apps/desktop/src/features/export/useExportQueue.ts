import { useCallback, useEffect, useState } from "react";
import * as ipc from "../../ipc/client";
import type { ExportBatchDto } from "../../ipc/generated/ExportBatchDto";
import type { ExportQueueEvent } from "../../ipc/generated/ExportQueueEvent";

export type ExportProgress = Extract<ExportQueueEvent, { type: "progress" }>;
export type ExportFinished = Extract<ExportQueueEvent, { type: "finished" }>;

/** What to say when a run ends ("Exported 12 photos to Lake District"). */
export function finishedMessage(f: ExportFinished): string {
  const photos = (n: number) => `${n} photo${n === 1 ? "" : "s"}`;
  const folder = f.folder ? f.folder.split(/[\\/]/).filter(Boolean).pop() : null;
  const failed = f.failed.length > 0 ? ` · ${photos(f.failed.length)} couldn’t be exported` : "";
  if (f.cancelled) return f.exported > 0 ? `Export stopped after ${photos(f.exported)}` : "Export cancelled";
  if (f.exported === 0) return `Nothing exported${failed}`;
  return `Exported ${photos(f.exported)}${folder ? ` to ${folder}` : ""}${failed}`;
}

/**
 * The export queue (ADR 0050): photos export one after another in Rust while the
 * photographer keeps working; this follows its progress and reports the outcome.
 */
export function useExportQueue(onError: (e: unknown) => void, notify: (message: string) => void) {
  const [progress, setProgress] = useState<ExportProgress | null>(null);
  const [lastRun, setLastRun] = useState<ExportFinished | null>(null);

  useEffect(() => {
    const unlisten = ipc
      .onExportQueueEvent((e) => {
        if (e.type === "progress") {
          setProgress(e);
          return;
        }
        setProgress(null);
        setLastRun(e);
        notify(finishedMessage(e));
        if (e.failed.length > 0) console.warn("exports that failed", e.failed);
      })
      .catch((e: unknown) => {
        onError(e);
        return () => {};
      });
    return () => void unlisten.then((u) => u());
  }, [onError, notify]);

  const start = useCallback(
    async (batch: ExportBatchDto) => {
      try {
        const total = await ipc.startExport(batch);
        // Shown at once, before the first photo reports.
        setProgress((p) => p ?? { type: "progress", done: 0, total, current: "", fraction: 0 });
        return true;
      } catch (e) {
        onError(e);
        return false;
      }
    },
    [onError],
  );

  return { progress, lastRun, start, cancel: () => void ipc.cancelExports().catch(onError) };
}

export type ExportQueueApi = ReturnType<typeof useExportQueue>;
