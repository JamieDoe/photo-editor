/**
 * Typed wrappers around Tauri commands. All payload types are generated from Rust
 * (see ./generated), so this file only names commands and adapts binary responses.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ChromaticAberration } from "./generated/ChromaticAberration";
import type { EngineInfoDto } from "./generated/EngineInfoDto";
import type { BackupStatusDto } from "./generated/BackupStatusDto";
import type { ClientErrorReport } from "./generated/ClientErrorReport";
import type { CollectionCountsDto } from "./generated/CollectionCountsDto";
import type { CollectionKindDto } from "./generated/CollectionKindDto";
import type { CollectionListingDto } from "./generated/CollectionListingDto";
import type { DiagnosticsDto } from "./generated/DiagnosticsDto";
import type { EditRecipe } from "./generated/EditRecipe";
import type { EditSavedDto } from "./generated/EditSavedDto";
import type { ExportEvent } from "./generated/ExportEvent";
import type { ExportRequestDto } from "./generated/ExportRequestDto";
import type { ExportStartedDto } from "./generated/ExportStartedDto";
import type { FolderListingDto } from "./generated/FolderListingDto";
import type { ImageSummaryDto } from "./generated/ImageSummaryDto";
import type { IndexEvent } from "./generated/IndexEvent";
import type { LibraryStatusDto } from "./generated/LibraryStatusDto";
import type { MarkChangeDto } from "./generated/MarkChangeDto";
import type { IpcError } from "./generated/IpcError";
import type { PreviewRequestDto } from "./generated/PreviewRequestDto";
import type { QuitRequestedDto } from "./generated/QuitRequestedDto";
import type { SelfTestConfigDto } from "./generated/SelfTestConfigDto";
import type { Settings } from "./generated/Settings";
import type { SettingsViewDto } from "./generated/SettingsViewDto";
import { decodeFrame, type PreviewFrame } from "./frame";

/** Must match `EXPORT_EVENT` in src-tauri/src/ipc.rs. */
const EXPORT_EVENT = "export://event";
/** Must match `INDEX_EVENT` in src-tauri/src/ipc.rs. */
const INDEX_EVENT = "library://index";
/** Must match `QUIT_REQUESTED_EVENT` in src-tauri/src/lifecycle.rs. */
const QUIT_REQUESTED_EVENT = "app://quit-requested";

export const engineInfo = () => invoke<EngineInfoDto>("engine_info");

/** Binary responses arrive as ArrayBuffer; fall back if the transport serialised them. */
function toArrayBuffer(raw: ArrayBuffer | number[]): ArrayBuffer {
  return raw instanceof ArrayBuffer ? raw : new Uint8Array(raw).buffer;
}

/** Shows the native file dialog and opens the chosen photo (null if cancelled). */
export const openImageDialog = () => invoke<ImageSummaryDto | null>("open_image_dialog");

export const openImagePath = (path: string) => invoke<ImageSummaryDto>("open_image_path", { path });

export async function renderPreview(request: PreviewRequestDto): Promise<PreviewFrame> {
  return decodeFrame(toArrayBuffer(await invoke<ArrayBuffer | number[]>("render_preview", { request })));
}

/** A synthetic cancellation, for responses that became obsolete on the UI side. */
export const staleError = (): IpcError => ({ kind: "cancelled", message: "stale", reference: null });

export const exportImage = (request: ExportRequestDto) =>
  invoke<ExportStartedDto | null>("export_image", { request });

/** Async so that failures (including synchronous ones outside Tauri) become rejections. */
export async function onExportEvent(handler: (e: ExportEvent) => void): Promise<UnlistenFn> {
  return listen<ExportEvent>(EXPORT_EVENT, (event) => handler(event.payload));
}

export const getSettings = () => invoke<SettingsViewDto>("get_settings");

/** Saves settings; the result holds the values actually stored (clamped). */
export const updateSettings = (settings: Settings) => invoke<SettingsViewDto>("update_settings", { settings });

/** Native folder picker; grants and lists the folder. Null if the user cancelled. */
export const chooseFolder = () => invoke<FolderListingDto | null>("choose_folder");

/** Lists a folder inside a previously granted folder. */
export const listFolder = (path: string) => invoke<FolderListingDto>("list_folder", { path });

/** Indexes the library folder containing `path` in the background (see onIndexEvent). */
export const indexLibraryFolder = (path: string) => invoke<void>("index_library_folder", { path });

export const libraryStatus = () => invoke<LibraryStatusDto>("library_status");

/** Rates or flags photos; resolves to the new library-wide collection counts. */
export const setPhotoMarks = (paths: string[], change: MarkChangeDto) =>
  invoke<CollectionCountsDto>("set_photo_marks", { paths, change });

/** Saves a library photo's edit (a default recipe removes it). */
export const saveEdit = (path: string, recipe: EditRecipe) => invoke<EditSavedDto>("save_edit", { path, recipe });
/** Auto level: the straighten angle that levels the open photo, or null (no clear horizon). */
export const autoLevel = (imageId: number) => invoke<number | null>("auto_level", { imageId });
/** Remove chromatic aberration: the correction measured on the open photo, or null (too
 *  few clean edges to measure). */
export const measureChromaticAberration = (imageId: number) =>
  invoke<ChromaticAberration | null>("measure_chromatic_aberration", { imageId });
/** The Light section's tone curve for `recipe`: display values of evenly spaced tones. */
export const toneCurve = (recipe: EditRecipe) => invoke<number[]>("tone_curve", { recipe });

export const libraryBackups = () => invoke<BackupStatusDto>("library_backups");
export const backUpLibrary = () => invoke<BackupStatusDto>("back_up_library");
export const showBackups = () => invoke<void>("show_backups");
/** Native folder dialog; null if cancelled. */
export const chooseBackupCopyFolder = () => invoke<BackupStatusDto | null>("choose_backup_copy_folder");
export const stopBackupCopies = () => invoke<BackupStatusDto>("stop_backup_copies");

export const libraryCollection = (kind: CollectionKindDto) =>
  invoke<CollectionListingDto>("library_collection", { kind });

/** A photo's thumbnail as JPEG bytes (long edge at most 512 px). */
export async function libraryThumbnail(path: string): Promise<ArrayBuffer> {
  return toArrayBuffer(await invoke<ArrayBuffer | number[]>("library_thumbnail", { path }));
}

/** Cancels a pending `libraryThumbnail` request; it then rejects as cancelled. */
export const cancelThumbnail = (path: string) => invoke<void>("cancel_thumbnail", { path });

export async function onIndexEvent(handler: (e: IndexEvent) => void): Promise<UnlistenFn> {
  return listen<IndexEvent>(INDEX_EVENT, (event) => handler(event.payload));
}

export const setDefaultFolder = (path: string) => invoke<SettingsViewDto>("set_default_folder", { path });

export const diagnostics = () => invoke<DiagnosticsDto>("diagnostics");

export const openLogsFolder = () => invoke<void>("open_logs_folder");

/** Writes a UI-side error to the local log; returns its reference (null if throttled). */
export const reportClientError = (report: ClientErrorReport) =>
  invoke<string | null>("report_client_error", { report });

/** Closing or quitting was held because work is running; the UI should confirm. */
export async function onQuitRequested(handler: (e: QuitRequestedDto) => void): Promise<UnlistenFn> {
  return listen<QuitRequestedDto>(QUIT_REQUESTED_EVENT, (event) => handler(event.payload));
}

/** Quits after cancelling running exports (the user confirmed). */
export const quit = () => invoke<void>("quit");

export const selfTestGrantFolder = () => invoke<string | null>("self_test_grant_folder");

export const selfTestRequestClose = () => invoke<void>("self_test_request_close");

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
