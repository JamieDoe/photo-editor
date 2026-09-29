/**
 * Dev-only UI harness: runs the React app in a normal browser with Tauri IPC mocked,
 * for layout work without the Rust side. Not part of the production build (only
 * index.html is bundled). The placeholder frame is a flat test pattern, not a render.
 *
 *   npm run dev  ->  http://localhost:1420/dev/mock.html
 */
import { mockIPC } from "@tauri-apps/api/mocks";
import { createRoot } from "react-dom/client";
import { App } from "../src/app/App";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "../src/styles.css";

const specs = [
  { key: "exposure", label: "Exposure", group: "Light", min: -5, max: 5, step: 0.01, default: 0 },
  { key: "contrast", label: "Contrast", group: "Light", min: -100, max: 100, step: 1, default: 0 },
  { key: "temperature", label: "Temperature", group: "Colour", min: -100, max: 100, step: 1, default: 0 },
  { key: "saturation", label: "Saturation", group: "Colour", min: -100, max: 100, step: 1, default: 0 },
];

let mockSettings: Record<string, unknown> = {
  version: 1,
  general: { theme: "system" },
  performance: { previewCacheMb: 256, backgroundIntensity: "balanced" },
  library: { defaultFolder: null, recentFolders: [] },
  export: { jpegQuality: 92 },
};

/** Marks by photo path; seeded with a few, updated by set_photo_marks. */
let mockBackups = { enabled: true, count: 5, totalBytes: 11_800_000, latestAtMs: Date.now() - 2 * 3600_000, folder: "/Users/me/Library/Application Support/app/backups" };
const mockEdits = new Map<string, Record<string, number>>();
let openedPath = "/mock.nef";
const mockMarks = new Map<string, { rating: number; flag: "none" | "pick" | "reject" }>();
const marksOf = (path: string, i: number) =>
  mockMarks.get(path) ?? { rating: i % 9 === 0 ? 4 : i % 13 === 0 ? 2 : 0, flag: i % 7 === 0 ? ("pick" as const) : i % 17 === 0 ? ("reject" as const) : ("none" as const) };
let lastListing: ReturnType<typeof mockListing> | null = null;

function mockListing(path: string) {
  const parts = path.split("/").filter(Boolean);
  const rootIndex = parts.indexOf("Photos");
  const crumbs = parts.slice(rootIndex).map((name, i) => ({ name, path: "/" + parts.slice(0, rootIndex + i + 1).join("/") }));
  const photos = Array.from({ length: 2400 }, (_, i) => ({
    name: `DSC_${String(i * 7 + 2).padStart(4, "0")}.${i % 5 === 4 ? "JPG" : "NEF"}`,
    path: `${path}/DSC_${i}.NEF`,
    sizeBytes: 24_000_000 + i * 731_000,
    modifiedMs: Date.UTC(2026, 7, 14, 9, i % 60),
    raw: i % 5 !== 4,
    marks: marksOf(`${path}/DSC_${i}.NEF`, i),
    edited: mockEdits.has(`${path}/DSC_${i}.NEF`) || i % 10 === 3,
    details: i % 3 === 2 ? null : { camera: "Nikon Z 6", lens: "NIKKOR Z 24-70mm f/4 S", capturedAt: `2026-09-${24 + (i % 3)}T06:${String(10 + (i % 50)).padStart(2, "0")}:12`, iso: 100, aperture: 8, shutterSeconds: 1 / 125, focalLengthMm: 35, width: 6048, height: 4024 },
  }));
  return {
    path,
    name: parts[parts.length - 1],
    breadcrumbs: crumbs,
    folders: parts.length - rootIndex < 3 ? [{ name: "Day 1", path: `${path}/Day 1` }, { name: "Day 2", path: `${path}/Day 2` }] : [],
    photos,
    skipped: 0,
  };
}

function placeholderFrame(w: number, h: number): ArrayBuffer {
  const buf = new ArrayBuffer(20 + w * h * 4);
  const v = new DataView(buf);
  v.setUint32(0, w, true);
  v.setUint32(4, h, true);
  v.setUint32(8, 1, true);
  v.setFloat32(16, 2.1, true);
  const px = new Uint8Array(buf, 20);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const i = (y * w + x) * 4;
    px[i] = (x * 255) / w; px[i + 1] = (y * 255) / h; px[i + 2] = 128; px[i + 3] = 255;
  }
  return buf;
}

/** A gradient "photo" as JPEG bytes; every fourth one is portrait. Arrives after a
 *  short random delay, like a thumbnail being generated. */
async function mockThumbnail(path: string): Promise<ArrayBuffer> {
  const n = Number(/_(\d+)\./.exec(path)?.[1] ?? 0);
  const [w, h] = n % 4 === 3 ? [341, 512] : [512, 341];
  const canvas = new OffscreenCanvas(w, h);
  const g = canvas.getContext("2d")!;
  const grad = g.createLinearGradient(0, 0, w, h);
  grad.addColorStop(0, `hsl(${(n * 47) % 360} 45% 35%)`);
  grad.addColorStop(1, `hsl(${(n * 47 + 60) % 360} 50% 70%)`);
  g.fillStyle = grad;
  g.fillRect(0, 0, w, h);
  await new Promise((r) => setTimeout(r, 20 + Math.random() * 150));
  return (await canvas.convertToBlob({ type: "image/jpeg", quality: 0.8 })).arrayBuffer();
}

function counts() {
  const all = (lastListing ?? mockListing("/Users/me/Photos/2026 Iceland")).photos.map((p, i) => marksOf(p.path, i));
  return {
    picks: all.filter((m) => m.flag === "pick").length,
    rated: all.filter((m) => m.rating > 0).length,
    rejected: all.filter((m) => m.flag === "reject").length,
  };
}

mockIPC((cmd, payload) => {
  switch (cmd) {
    case "engine_info":
      return { rendererVersion: 1, recipeVersion: 1, decoders: ["zune-jpeg", "libraw"], extensions: [], librawVersion: "mock", renderBackend: "cpu", jpegEncoder: "libjpeg-turbo", embeddedJpegDecoder: "libjpeg-turbo (DCT-scaled)", cpuThreads: 10, adjustments: specs };
    case "open_image_dialog":
    case "open_image_path":
      openedPath = cmd === "open_image_path" ? (payload as { path: string }).path : "/elsewhere/mock.nef";
      return {
        path: openedPath,
        savedRecipe: mockEdits.get(openedPath) ?? null,
        editSaving: cmd === "open_image_path" ? "library" : "notInLibrary", id: 1, fileName: "mock.nef", decoder: "libraw", cameraRaw: true, camera: "Mock Camera", iso: 100, aperture: 6.7, shutterSeconds: 1, focalLengthMm: 52, fullWidth: 6000, fullHeight: 4000, levels: [[3000, 2000], [1500, 1000], [750, 500], [375, 250]], pyramidBytes: 0, identityMs: 0.5, decodeMs: 380, pyramidMs: 2, embeddedPreviewMs: 12 };
    case "render_preview":
      return placeholderFrame(600, 400);
    case "self_test_config":
      return null;
    case "diagnostics":
      return { appVersion: "0.0.1", os: "mock", arch: "mock", cpuThreads: 10, rendererVersion: 1, librawVersion: "mock", jpegEncoder: "libjpeg-turbo", embeddedJpegDecoder: "libjpeg-turbo (DCT-scaled)", logDir: "/mock/logs" };
    case "report_client_error":
      return "E-MOCK-1";
    case "update_settings":
      mockSettings = (payload as { settings: Record<string, unknown> }).settings;
      return { settings: mockSettings, restartRequired: false, recoveredFrom: null };
    case "choose_folder": {
      const lib = mockSettings.library as { recentFolders: string[] };
      lib.recentFolders = ["/Users/me/Photos/2026 Iceland", ...lib.recentFolders.filter((f) => f !== "/Users/me/Photos/2026 Iceland")];
      lastListing = mockListing("/Users/me/Photos/2026 Iceland");
      return lastListing;
    }
    case "index_library_folder":
      return null;
    case "library_status":
      return { photos: 1284, folders: ["/Users/me/Photos/2026 Iceland"], notice: null, collections: counts() };
    case "list_folder":
      lastListing = mockListing((payload as { path: string }).path);
      return lastListing;
    case "set_photo_marks": {
      const { paths, change } = payload as { paths: string[]; change: { type: "rating"; stars: number } | { type: "flag"; flag: "none" | "pick" | "reject" } };
      for (const p of paths) {
        const i = lastListing?.photos.findIndex((x) => x.path === p) ?? -1;
        const m = { ...marksOf(p, i) };
        if (change.type === "rating") m.rating = change.stars;
        else m.flag = change.flag;
        mockMarks.set(p, m);
      }
      return counts();
    }
    case "save_edit": {
      const { path, recipe } = payload as { path: string; recipe: Record<string, number> };
      const edited = ["exposure", "contrast", "temperature", "saturation"].some((k) => recipe[k] !== 0);
      if (edited) mockEdits.set(path, recipe);
      else mockEdits.delete(path);
      return new Promise((r) => setTimeout(() => r({ edited }), 150));
    }
    case "library_backups":
      return mockBackups;
    case "back_up_library":
      mockBackups = { ...mockBackups, count: Math.min(mockBackups.count + 1, 14), latestAtMs: Date.now(), totalBytes: mockBackups.totalBytes + 2_400_000 };
      return new Promise((r) => setTimeout(() => r(mockBackups), 600));
    case "show_backups":
      return null;
    case "library_collection": {
      const kind = (payload as { kind: "picks" | "rated" | "rejected" }).kind;
      const all = lastListing ?? mockListing("/Users/me/Photos/2026 Iceland");
      const photos = all.photos
        .map((p, i) => ({ ...p, marks: marksOf(p.path, i) }))
        .filter((p) => (kind === "picks" ? p.marks.flag === "pick" : kind === "rejected" ? p.marks.flag === "reject" : p.marks.rating > 0));
      return { kind, photos };
    }
    case "library_thumbnail":
      return mockThumbnail((payload as { path: string }).path);
    case "cancel_thumbnail":
      return null;
    case "set_default_folder":
      (mockSettings.library as { defaultFolder: string | null }).defaultFolder = (payload as { path: string }).path;
      return { settings: mockSettings, restartRequired: false, recoveredFrom: null };
    case "get_settings":
      return {
        settings: mockSettings,
        restartRequired: false,
        recoveredFrom: null,
      };
    default:
      return null;
  }
}, { shouldMockEvents: true });

createRoot(document.getElementById("root")!).render(<App />);
