/**
 * Typed wrappers around Tauri commands. All payload types are generated from Rust
 * (see ./generated), so this file only names commands and adapts binary responses.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ExportEstimateDto } from "./generated/ExportEstimateDto";
import type { ExportEstimateRequestDto } from "./generated/ExportEstimateRequestDto";
import type { LibraryPlace } from "./generated/LibraryPlace";
import type { ChromaticAberration } from "./generated/ChromaticAberration";
import type { EngineInfoDto } from "./generated/EngineInfoDto";
import type { BackupStatusDto } from "./generated/BackupStatusDto";
import type { ClientErrorReport } from "./generated/ClientErrorReport";
import type { CollectionCountsDto } from "./generated/CollectionCountsDto";
import type { CollectionKindDto } from "./generated/CollectionKindDto";
import type { AlbumDto } from "./generated/AlbumDto";
import type { AlbumListingDto } from "./generated/AlbumListingDto";
import type { CollectionListingDto } from "./generated/CollectionListingDto";
import type { SearchResultsDto } from "./generated/SearchResultsDto";
import type { DiagnosticsDto } from "./generated/DiagnosticsDto";
import type { EditRecipe } from "./generated/EditRecipe";
import type { EditSavedDto } from "./generated/EditSavedDto";
import type { ExportBatchDto } from "./generated/ExportBatchDto";
import type { ExportEvent } from "./generated/ExportEvent";
import type { ExportQueueEvent } from "./generated/ExportQueueEvent";
import type { ExportRequestDto } from "./generated/ExportRequestDto";
import type { ExportStartedDto } from "./generated/ExportStartedDto";
import type { FolderListingDto } from "./generated/FolderListingDto";
import type { PastedEditsDto } from "./generated/PastedEditsDto";
import type { PresetDto } from "./generated/PresetDto";
import type { PresetImportDto } from "./generated/PresetImportDto";
import type { ImageSummaryDto } from "./generated/ImageSummaryDto";
import type { IndexEvent } from "./generated/IndexEvent";
import type { LibraryStatusDto } from "./generated/LibraryStatusDto";
import type { MarkChangeDto } from "./generated/MarkChangeDto";
import type { IpcError } from "./generated/IpcError";
import type { PreviewRequestDto } from "./generated/PreviewRequestDto";
import type { QuitRequestedDto } from "./generated/QuitRequestedDto";
import type { SelfTestConfigDto } from "./generated/SelfTestConfigDto";
import type { Settings } from "./generated/Settings";
import type { AutoTone } from "./generated/AutoTone";
import type { ToneSetting } from "./generated/ToneSetting";
import type { Removal } from "./generated/Removal";
import type { Spot } from "./generated/Spot";
import type { SpotKind } from "./generated/SpotKind";
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

/** Decodes the open photo at full resolution for zoom (ADR 0070); resolves when
 *  window renders can use it. */
export const prepareFull = (imageId: number) => invoke<null>("prepare_full", { imageId });

/** Fills the open photo's removals at full resolution (ADR 0070), so every view shows
 *  the same fill; resolves when renders use it. */
export const prepareFill = (imageId: number, removals: Removal[]) => invoke<null>("prepare_fill", { imageId, removals });

/** Auto tone (ADR 0071): the tone sliders as a starting point for the open photo as
 *  `recipe` edits it. */
export const autoTone = (imageId: number, recipe: EditRecipe) => invoke<AutoTone>("auto_tone", { imageId, recipe });

/** Auto for one setting (ADR 0071): its value for the open photo as `recipe` edits it,
 *  the rest of the edit as it is. */
export const autoSetting = (imageId: number, recipe: EditRecipe, setting: ToneSetting) =>
  invoke<number>("auto_setting", { imageId, recipe, setting });

/** A synthetic cancellation, for responses that became obsolete on the UI side. */
export const staleError = (): IpcError => ({ kind: "cancelled", message: "stale", reference: null });

export const exportImage = (request: ExportRequestDto) =>
  invoke<ExportStartedDto | null>("export_image", { request });

/** Async so that failures (including synchronous ones outside Tauri) become rejections. */
/** Chooses the export folder in the system's dialog (ADR 0050); null if cancelled. */
export const chooseExportFolder = () => invoke<string | null>("choose_export_folder");
/** Queues photos for export; resolves to the photos now in the run. */
export const startExport = (batch: ExportBatchDto) => invoke<number>("start_export", { batch });
export const cancelExports = () => invoke<null>("cancel_exports");
export async function onExportQueueEvent(handler: (e: ExportQueueEvent) => void): Promise<UnlistenFn> {
  return listen<ExportQueueEvent>("export://queue", (event) => handler(event.payload));
}

export async function onExportEvent(handler: (e: ExportEvent) => void): Promise<UnlistenFn> {
  return listen<ExportEvent>(EXPORT_EVENT, (event) => handler(event.payload));
}

export const getSettings = () => invoke<SettingsViewDto>("get_settings");

/** Saves settings; the result holds the values actually stored (clamped). */
/** An export's estimated size (ADR 0068), for the dialog. */
export const estimateExport = (request: ExportEstimateRequestDto) => invoke<ExportEstimateDto>("estimate_export", { request });
export const updateSettings = (settings: Settings) => invoke<SettingsViewDto>("update_settings", { settings });
/** Records where the Library is, to reopen it next launch (ADR 0065); folders are checked in Rust. */
export const rememberPlace = (place: LibraryPlace) => invoke<void>("remember_place", { place });

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
/** Sets the settings of `groups` from `source` on each library photo in `paths` (ADR
 *  0049): pasting or syncing onto photos that are not open. */
export const pasteEditsTo = (paths: string[], source: EditRecipe, groups: string[]) =>
  invoke<PastedEditsDto>("paste_edits_to", { paths, source, groups });

/** Presets (ADR 0046): the built-in looks, then the photographer's own. */
export const listPresets = () => invoke<PresetDto[]>("list_presets");
/** Saves the look of `recipe` as a preset. */
export const createPreset = (name: string, recipe: EditRecipe) => invoke<PresetDto>("create_preset", { name, recipe });
export const renamePreset = (id: string, name: string) => invoke<null>("rename_preset", { id, name });
/** Replaces a saved preset's look; resolves to the look stored. */
export const updatePreset = (id: string, recipe: EditRecipe) => invoke<EditRecipe>("update_preset", { id, recipe });
export const deletePreset = (id: string) => invoke<null>("delete_preset", { id });
/** Saves a preset as a file the photographer chooses (`destination` is for the
 *  self-test only); resolves to the path, or null if they cancelled. */
export const exportPreset = (id: string, destination: string | null = null) =>
  invoke<string | null>("export_preset", { id, destination });
/** Imports preset files the photographer chooses (the app's own, or Lightroom .xmp;
 *  `paths` is for the self-test only). */
export const importPresets = (paths: string[] | null = null) => invoke<PresetImportDto>("import_presets", { paths });
/** Auto level: the straighten angle that levels the open photo, or null (no clear horizon). */
export const autoLevel = (imageId: number) => invoke<number | null>("auto_level", { imageId });
/** A new heal or clone spot at `at` (photo fractions) of `radius` (a fraction of the
 *  long edge), its source found nearby clear of `avoid`; null when none fits. */
export const newSpot = (imageId: number, kind: SpotKind, at: [number, number], radius: number, avoid: Spot[]) =>
  invoke<Spot | null>("new_spot", { imageId, kind, at, radius, avoid });
/** Sensor dust on the open photo (ADR 0058): heal spots for it, clear of `existing`. */
export const findDust = (imageId: number, existing: Spot[]) => invoke<Spot[]>("find_dust", { imageId, existing });
/** Remove chromatic aberration: the correction measured on the open photo, or null (too
 *  few clean edges to measure). */
export const measureChromaticAberration = (imageId: number) =>
  invoke<ChromaticAberration | null>("measure_chromatic_aberration", { imageId });
/** The Light section's tone curve for `recipe`: display values of evenly spaced tones. */

export const libraryBackups = () => invoke<BackupStatusDto>("library_backups");
export const backUpLibrary = () => invoke<BackupStatusDto>("back_up_library");
export const showBackups = () => invoke<void>("show_backups");
/** Native folder dialog; null if cancelled. */
export const chooseBackupCopyFolder = () => invoke<BackupStatusDto | null>("choose_backup_copy_folder");
export const stopBackupCopies = () => invoke<BackupStatusDto>("stop_backup_copies");

/** Albums (ADR 0055): the photographer's own groups of photos, by name. */
export const listAlbums = () => invoke<AlbumDto[]>("list_albums");
/** A new album holding the photos at `paths` (which may be none). */
export const createAlbum = (name: string, paths: string[]) => invoke<AlbumDto>("create_album", { name, paths });
export const renameAlbum = (id: number, name: string) => invoke<AlbumDto>("rename_album", { id, name });
/** Deletes an album; its photos are untouched. */
export const deleteAlbum = (id: number) => invoke<void>("delete_album", { id });
export const addToAlbum = (id: number, paths: string[]) => invoke<AlbumDto>("add_to_album", { id, paths });
export const removeFromAlbum = (id: number, paths: string[]) => invoke<AlbumDto>("remove_from_album", { id, paths });
export const albumPhotos = (id: number) => invoke<AlbumListingDto>("album_photos", { id });
/** The library's photos matching every word of `query` (ADR 0056): file and folder
 *  names, camera, lens, capture date. */
export const searchLibrary = (query: string) => invoke<SearchResultsDto>("search_library", { query });
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
