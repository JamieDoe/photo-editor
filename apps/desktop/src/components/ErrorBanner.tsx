import { useState } from "react";
import { formatErrorDetails, type AppError } from "../app/errors";
import * as ipc from "../ipc/client";
import { copyText } from "../lib/clipboard";

interface Props {
  error: AppError;
  onDismiss: () => void;
}

/** A photographer-facing error with ways to get help: copy details, open logs. */
export function ErrorBanner({ error, onDismiss }: Props) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    const diag = await ipc.diagnostics().catch(() => null);
    setCopied(await copyText(formatErrorDetails(error, diag)));
  };
  return (
    <div className="error-banner" role="alert">
      <span className="error-message">{error.message}</span>
      {error.reference && <span className="error-ref">Ref {error.reference}</span>}
      <button onClick={() => void copy()}>{copied ? "Copied" : "Copy details"}</button>
      <button onClick={() => void ipc.openLogsFolder().catch(() => undefined)}>Open logs folder</button>
      <button aria-label="Dismiss" onClick={onDismiss}>
        ✕
      </button>
    </div>
  );
}
