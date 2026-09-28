/**
 * Typed wrappers around Tauri commands. All payload types are generated from Rust
 * (see ./generated), so this file only names commands and adapts binary responses.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { EngineInfoDto } from "./generated/EngineInfoDto";
import type { ExportEvent } from "./generated/ExportEvent";
import type { ExportRequestDto } from "./generated/ExportRequestDto";
import type { ExportStartedDto } from "./generated/ExportStartedDto";
import type { ImageSummaryDto } from "./generated/ImageSummaryDto";
import type { IpcError } from "./generated/IpcError";
import type { PreviewRequestDto } from "./generated/PreviewRequestDto";
import type { SelfTestConfigDto } from "./generated/SelfTestConfigDto";
import { decodeFrame, type PreviewFrame } from "./frame";

/** Must match `EXPORT_EVENT` in src-tauri/src/ipc.rs. */
const EXPORT_EVENT = "export://event";

export const engineInfo = () => invoke<EngineInfoDto>("engine_info");

/** Binary responses arrive as ArrayBuffer; fall back if the transport serialised them. */
function toArrayBuffer(raw: ArrayBuffer | number[]): ArrayBuffer {
  return raw instanceof ArrayBuffer ? raw : new Uint8Array(raw).buffer;
}

export type PreviewHandler = (frame: PreviewFrame) => void;

/** Channel on which Rust streams the file's embedded preview while it decodes. */
function previewChannel(onPreview: PreviewHandler): Channel<ArrayBuffer | number[]> {
  const channel = new Channel<ArrayBuffer | number[]>();
  channel.onmessage = (raw) => {
    try {
      onPreview(decodeFrame(toArrayBuffer(raw)));
    } catch (e) {
      console.error("bad embedded preview frame", e);
    }
  };
  return channel;
}

export const openImageDialog = (onPreview: PreviewHandler) =>
  invoke<ImageSummaryDto | null>("open_image_dialog", { onPreview: previewChannel(onPreview) });

export const openImagePath = (path: string, onPreview: PreviewHandler) =>
  invoke<ImageSummaryDto>("open_image_path", { path, onPreview: previewChannel(onPreview) });

export async function renderPreview(request: PreviewRequestDto): Promise<PreviewFrame> {
  return decodeFrame(toArrayBuffer(await invoke<ArrayBuffer | number[]>("render_preview", { request })));
}

/** A synthetic cancellation, for responses that became obsolete on the UI side. */
export const staleError = (): IpcError => ({ kind: "cancelled", message: "stale" });

export const exportImage = (request: ExportRequestDto) =>
  invoke<ExportStartedDto | null>("export_image", { request });

/** Async so that failures (including synchronous ones outside Tauri) become rejections. */
export async function onExportEvent(handler: (e: ExportEvent) => void): Promise<UnlistenFn> {
  return listen<ExportEvent>(EXPORT_EVENT, (event) => handler(event.payload));
}

export const selfTestConfig = () => invoke<SelfTestConfigDto | null>("self_test_config");

export const selfTestReport = (report: unknown) => invoke<void>("self_test_report", { report });

export function isIpcError(e: unknown): e is IpcError {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}

/** Superseded/cancelled requests are expected during interaction and not errors. */
export function isCancellation(e: unknown): boolean {
  return isIpcError(e) && e.kind === "cancelled";
}

export function errorMessage(e: unknown): string {
  if (isIpcError(e)) return e.message;
  return e instanceof Error ? e.message : String(e);
}
