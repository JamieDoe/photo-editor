/**
 * Error handling for the UI. Photographers see a plain-language message plus a
 * reference; the technical detail goes to the local log (never uploaded).
 */
import * as ipc from "../ipc/client";
import type { ClientErrorSource } from "../ipc/generated/ClientErrorSource";
import type { DiagnosticsDto } from "../ipc/generated/DiagnosticsDto";

export interface AppError {
  message: string;
  /** Matches a line in the log file; null if the error was not logged. */
  reference: string | null;
}

const GENERIC_MESSAGE = "Something went wrong. Please try again.";

/** Converts anything thrown into an AppError. Errors from Rust already carry a
 * user-facing message and reference; anything else is logged first. */
export async function toAppError(e: unknown, source: ClientErrorSource = "unhandledRejection"): Promise<AppError> {
  if (ipc.isIpcError(e)) return { message: e.message, reference: e.reference };
  return { message: GENERIC_MESSAGE, reference: await report(source, e) };
}

/** Logs a UI-side error locally; returns its reference. Never throws. */
export async function report(source: ClientErrorSource, e: unknown): Promise<string | null> {
  const err = e instanceof Error ? e : null;
  try {
    return await ipc.reportClientError({
      source,
      message: err ? `${err.name}: ${err.message}` : String(e),
      stack: err?.stack ?? null,
    });
  } catch {
    return null; // logging must never cause a second failure
  }
}

/** Routes uncaught errors and unhandled rejections to the local log. */
export function installGlobalErrorHandlers(): void {
  window.addEventListener("error", (event) => void report("uncaught", event.error ?? event.message));
  window.addEventListener("unhandledrejection", (event) => {
    // Superseded requests are expected; everything else is a bug worth logging.
    if (!ipc.isCancellation(event.reason)) void report("unhandledRejection", event.reason);
  });
}

/** Text for "Copy details": enough to diagnose from the log, nothing more. */
export function formatErrorDetails(error: AppError, diag: DiagnosticsDto | null, now: Date = new Date()): string {
  const lines = [`Message: ${error.message}`, `Reference: ${error.reference ?? "none"}`, `Time: ${now.toISOString()}`];
  if (diag) {
    lines.push(
      `App: ${diag.appVersion} · ${diag.os} ${diag.arch} · ${diag.cpuThreads} threads`,
      `Renderer: v${diag.rendererVersion} · LibRaw ${diag.librawVersion ?? "n/a"} · ${diag.jpegEncoder} · ${diag.embeddedJpegDecoder}`,
    );
    if (diag.logDir) lines.push(`Logs: ${diag.logDir}`);
  }
  return lines.join("\n");
}
