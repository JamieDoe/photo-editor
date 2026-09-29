import { useEffect, useState } from "react";
import * as ipc from "../../ipc/client";
import type { BackupStatusDto } from "../../ipc/generated/BackupStatusDto";
import { formatBytes, formatDateTime } from "../../lib/format";

/**
 * Library backups (ADR 0021): the app backs up the library database by itself; this
 * shows when it last did, and offers "Back up now" and the folder.
 */
export function BackupSettings() {
  const [status, setStatus] = useState<BackupStatusDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc.libraryBackups().then(setStatus, (e: unknown) => setError(ipc.errorMessage(e)));
  }, []);

  const backUpNow = async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await ipc.backUpLibrary());
    } catch (e) {
      setError(ipc.errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const summary = !status
    ? "…"
    : !status.enabled
      ? "This session’s library is temporary, so it isn’t backed up."
      : status.latestAtMs === null
        ? "No backup yet. The first one is made shortly after start-up."
        : `Last backup ${formatDateTime(status.latestAtMs)} · ${status.count} kept (${formatBytes(status.totalBytes)})`;

  return (
    <section className="settings-group">
      <h2>Library backups</h2>
      <div className="settings-card">
        <div className="setting">
          <div className="setting-text">
            <span className="setting-name">Ratings, flags and edits</span>
            <small>
              Backed up automatically on this computer: regularly while you work, and before every app upgrade. If the
              library is ever damaged, the newest good backup is restored.
            </small>
            <small className="setting-status" aria-live="polite">
              {error ?? summary}
            </small>
          </div>
          <div className="setting-actions">
            <button onClick={() => void backUpNow()} disabled={busy || !status?.enabled}>
              {busy ? "Backing up…" : "Back up now"}
            </button>
            <button className="ghost" onClick={() => void ipc.showBackups().catch((e: unknown) => setError(ipc.errorMessage(e)))}>
              Show in Finder
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}
