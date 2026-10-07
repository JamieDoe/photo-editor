/**
 * Dev-only UI harness: runs the React app in a normal browser with Tauri IPC mocked,
 * for layout work without the Rust side. Not part of the production build (only
 * index.html is bundled). The placeholder frame is a flat test pattern, not a render.
 *
 *   npm run dev  ->  http://localhost:1420/dev/mock.html
 */
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { createRoot } from "react-dom/client";
import { App } from "../src/app/App";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "../src/styles.css";

const mixerControl = { group: "Colour mixer", min: -100, max: 100, step: 1, default: 0, more: true, unit: "" };
const mixerSpec = {
  bands: [["red", "Reds"], ["orange", "Oranges"], ["yellow", "Yellows"], ["green", "Greens"], ["aqua", "Aquas"], ["blue", "Blues"], ["purple", "Purples"], ["magenta", "Magentas"]].map(([key, label]) => ({ key, label })),
  controls: [{ key: "hue", label: "Hue", ...mixerControl }, { key: "saturation", label: "Saturation", ...mixerControl }, { key: "luminance", label: "Luminance", ...mixerControl }],
};
const specs = [
  { key: "exposure", label: "Exposure", group: "Light", min: -5, max: 5, step: 0.01, default: 0, more: false, unit: "EV" },
  { key: "contrast", label: "Contrast", group: "Light", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "highlights", label: "Highlights", group: "Light", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "shadows", label: "Shadows", group: "Light", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "whites", label: "Whites", group: "Light", min: -100, max: 100, step: 1, default: 0, more: true, unit: "" },
  { key: "blacks", label: "Blacks", group: "Light", min: -100, max: 100, step: 1, default: 0, more: true, unit: "" },
  { key: "dehaze", label: "Dehaze", group: "Light", min: -100, max: 100, step: 1, default: 0, more: true, unit: "" },
  { key: "temperature", label: "Temperature", group: "Colour", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "tint", label: "Tint", group: "Colour", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "vibrance", label: "Vibrance", group: "Colour", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "saturation", label: "Saturation", group: "Colour", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "texture", label: "Texture", group: "Detail", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "clarity", label: "Clarity", group: "Detail", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "sharpening", label: "Sharpening", group: "Detail", min: 0, max: 150, step: 1, default: 40, more: false, unit: "" },
  { key: "noiseReduction", label: "Noise reduction", group: "Detail", min: 0, max: 100, step: 1, default: 0, more: false, unit: "" },
  { key: "vignette", label: "Vignette", group: "Detail", min: -100, max: 100, step: 1, default: 0, more: true, unit: "" },
  { key: "grain", label: "Grain", group: "Detail", min: 0, max: 100, step: 1, default: 0, more: true, unit: "" },
];

let mockSettings: Record<string, unknown> = {
  version: 1,
  general: { theme: "system" },
  performance: { previewCacheMb: 256, backgroundIntensity: "balanced" },
  library: { defaultFolder: null, recentFolders: [] },
  export: { format: "jpeg", sharpen: "screen", colourSpace: "srgb", keepMetadata: true, stripLocation: false, jpegQuality: 85, folder: null, longEdge: 2048, preset: "web" },
  backups: { copyFolder: null },
};

/** Marks by photo path; seeded with a few, updated by set_photo_marks. */
let mockBackups = { enabled: true, count: 5, totalBytes: 11_800_000, latestAtMs: Date.now() - 2 * 3600_000, folder: "/Users/me/Library/Application Support/app/backups", copy: null as null | { folder: string; connected: boolean; count: number; latestAtMs: number | null } };
const mockEdits = new Map<string, Record<string, number>>();
let openedPath = "/mock.nef";
const mockMarks = new Map<string, { rating: number; flag: "none" | "pick" | "reject" }>();
/** Dev-only albums: name and member paths, by id. */
const mockAlbums = new Map<number, { name: string; paths: string[] }>([[1, { name: "Portfolio", paths: [] }]]);
let nextAlbum = 2;
const albumDto = (id: number) => {
  const a = mockAlbums.get(id)!;
  return { id, name: a.name, count: a.paths.length, cover: a.paths[0] ?? null };
};
const albumList = () => [...mockAlbums.keys()].map(albumDto).sort((x, y) => x.name.localeCompare(y.name));
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
  // Header, histogram (flag 2: 4 planes of 256 u32), pixels.
  const hist = 4 * 256 * 4;
  const buf = new ArrayBuffer(28 + hist + w * h * 4);
  const v = new DataView(buf);
  v.setUint32(0, w, true);
  v.setUint32(4, h, true);
  v.setUint32(8, 1, true);
  v.setUint32(12, 2, true);
  v.setFloat32(16, 2.1, true);
  v.setUint32(20, w * 4, true);
  v.setUint32(24, h * 4, true);
  const px = new Uint8Array(buf, 28 + hist);
  const counts = new Uint32Array(4 * 256);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const i = (y * w + x) * 4;
    px[i] = (x * 255) / w; px[i + 1] = (y * 255) / h; px[i + 2] = 128; px[i + 3] = 255;
    counts[px[i]!]!++;
    counts[256 + px[i + 1]!]!++;
    counts[512 + px[i + 2]!]!++;
    counts[768 + ((54 * px[i]! + 183 * px[i + 1]! + 19 * px[i + 2]! + 128) >> 8)]!++;
  }
  counts.forEach((n, k) => v.setUint32(28 + k * 4, n, true));
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
    recent: 64,
  };
}

/** Dev-only presets: the built-in names (their recipes come from Rust in the app) and
 *  any saved in this page. */
const neutral = { version: 20, exposure: 0, contrast: 0, highlights: 0, shadows: 0, whites: 0, blacks: 0, dehaze: 0, temperature: 0, tint: 0, vibrance: 0, saturation: 0, texture: 0, clarity: 0, sharpening: 40, noiseReduction: 0, vignette: 0, grain: 0, look: "standard" };
let mockPresets = [
  ["natural", "Natural", { vibrance: 12, contrast: 8, shadows: 10 }],
  ["vivid", "Vivid", { contrast: 25, vibrance: 35, saturation: 10, clarity: 15 }],
  ["warm-film", "Warm film", { temperature: 30, contrast: -10, blacks: 25, saturation: -10, vignette: -25, highlights: -20 }],
  ["matte", "Matte", { contrast: -25, blacks: 40, saturation: -15 }],
  ["mono", "Mono", { saturation: -100, contrast: 30, clarity: 20 }],
  ["cool-fade", "Cool fade", { temperature: -30, tint: 6, blacks: 30, contrast: -15 }],
].map(([id, name, v]) => ({ id: `builtin:${id as string}`, name: name as string, builtIn: true, recipe: { ...neutral, ...(v as object) } as Record<string, unknown> }));
let nextPresetId = 1;
const lookOnly = (r: Record<string, unknown>) => {
  const { geometry: _g, chromaticAberration: _c, masks: _m, ...look } = r;
  return { ...look, exposure: 0 };
};

mockIPC((cmd, payload) => {
  switch (cmd) {
    case "list_presets":
      return mockPresets;
    case "create_preset": {
      const { name, recipe } = payload as { name: string; recipe: Record<string, unknown> };
      const p = { id: `user:${nextPresetId++}`, name: name.trim(), builtIn: false, recipe: lookOnly(recipe) };
      mockPresets = [...mockPresets, p];
      return p;
    }
    case "rename_preset": {
      const { id, name } = payload as { id: string; name: string };
      mockPresets = mockPresets.map((p) => (p.id === id ? { ...p, name: name.trim() } : p));
      return null;
    }
    case "update_preset": {
      const { id, recipe } = payload as { id: string; recipe: Record<string, unknown> };
      mockPresets = mockPresets.map((p) => (p.id === id ? { ...p, recipe: lookOnly(recipe) } : p));
      return lookOnly(recipe);
    }
    case "export_preset":
      return new Promise((r) => setTimeout(() => r("/Users/me/Desktop/preset.preset"), 200));
    case "import_presets": {
      // Dev-only stand-in: a Lightroom preset with settings left out, and a bad file.
      const p = { id: `user:${nextPresetId++}`, name: "Soft & Warm", builtIn: false, recipe: { ...neutral, contrast: 18, highlights: -42, shadows: 30, vibrance: 22 } };
      mockPresets = [...mockPresets, p];
      return new Promise((r) =>
        setTimeout(() => r({ imported: [{ preset: p, fromLightroom: true, leftOut: ["Parametric curve", "Color Grading", "Masks and healing"] }], failed: [{ file: "notes.xmp", message: "“notes.xmp” isn’t a preset this app can read." }] }), 300),
      );
    }
    case "delete_preset":
      mockPresets = mockPresets.filter((p) => p.id !== (payload as { id: string }).id);
      return null;
    case "engine_info":
      return { rendererVersion: 3, recipeVersion: 20, decoders: ["zune-jpeg", "libraw"], extensions: [], librawVersion: "mock", renderBackend: "cpu", jpegEncoder: "libjpeg-turbo", embeddedJpegDecoder: "libjpeg-turbo (DCT-scaled)", cpuThreads: 10, adjustments: specs, mixer: mixerSpec, straighten: { key: "straighten", label: "Straighten", group: "Geometry", min: -15, max: 15, step: 0.1, default: 0, more: false, unit: "°" }, perspective: ["vertical", "horizontal"].map((key) => ({ key, label: key === "vertical" ? "Vertical" : "Horizontal", group: "Geometry", min: -100, max: 100, step: 1, default: 0, more: true, unit: "" })), mask: [{ key: "exposure", label: "Exposure", group: "Mask", min: -2, max: 2, step: 0.01, default: 0, more: false, unit: "EV" }, { key: "warmth", label: "Warmth", group: "Mask", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" }, { key: "clarity", label: "Clarity", group: "Mask", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" }], maskFeather: { key: "feather", label: "Feather", group: "Mask", min: 0, max: 100, step: 1, default: 50, more: false, unit: "" }, maskDensity: { key: "density", label: "Density", group: "Mask", min: 0, max: 100, step: 1, default: 100, more: false, unit: "" }, settingGroups: [
        { id: "exposure", label: "Exposure", fields: ["exposure"], copiedByDefault: true },
        { id: "light", label: "Light and tone curve", fields: ["look", "contrast", "highlights", "shadows", "whites", "blacks", "dehaze", "pointCurve", "channelCurves"], copiedByDefault: true },
        { id: "whiteBalance", label: "White balance", fields: ["temperature", "tint"], copiedByDefault: true },
        { id: "colour", label: "Colour", fields: ["vibrance", "saturation", "mixer"], copiedByDefault: true },
        { id: "calibration", label: "Calibration", fields: ["calibration"], copiedByDefault: true },
        { id: "detail", label: "Detail and effects", fields: ["texture", "clarity", "sharpening", "noiseReduction", "vignette", "grain"], copiedByDefault: true },
        { id: "geometry", label: "Crop, geometry and lens", fields: ["geometry", "chromaticAberration"], copiedByDefault: false },
        { id: "masks", label: "Masks", fields: ["masks"], copiedByDefault: false },
        { id: "retouch", label: "Retouch", fields: ["spots"], copiedByDefault: false },
      ], curveRegions: [["highlights", "Highlights"], ["lights", "Lights"], ["darks", "Darks"], ["shadows", "Shadows"]].map(([key, label]) => ({ key, label, group: "Tone curve", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" })), grading: [
        { key: "luminance", label: "Luminance", group: "Colour grading", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
        { key: "blending", label: "Blending", group: "Colour grading", min: 0, max: 100, step: 1, default: 50, more: false, unit: "" },
        { key: "balance", label: "Balance", group: "Colour grading", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" },
      ], calibration: [
        ["shadowTint", "Tint", "Shadows"], ["redHue", "Hue", "Red primary"], ["redSaturation", "Saturation", "Red primary"],
        ["greenHue", "Hue", "Green primary"], ["greenSaturation", "Saturation", "Green primary"],
        ["blueHue", "Hue", "Blue primary"], ["blueSaturation", "Saturation", "Blue primary"],
      ].map(([key, label, group]) => ({ key, label, group, min: -100, max: 100, step: 1, default: 0, more: false, unit: "" })) };
    case "open_image_dialog":
    case "open_image_path":
      openedPath = cmd === "open_image_path" ? (payload as { path: string }).path : "/elsewhere/mock.nef";
      return {
        path: openedPath,
        savedRecipe: mockEdits.get(openedPath) ?? null,
        editSaving: cmd === "open_image_path" ? "library" : "notInLibrary", id: 1, fileName: "mock.nef", decoder: "libraw", cameraRaw: true, camera: "Mock Camera", iso: 100, aperture: 6.7, shutterSeconds: 1, focalLengthMm: 52, temperatureScale: { asShotKelvin: 5200, asShotTint: 6, miredPerUnit: 1.2, minKelvin: 1667, maxKelvin: 25000 }, fullWidth: 6000, fullHeight: 4000, levels: [[3000, 2000], [1500, 1000], [750, 500], [375, 250]], pyramidBytes: 0, identityMs: 0.5, decodeMs: 380, pyramidMs: 2, embeddedPreviewMs: 12 };
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
    case "find_dust":
      // Dev-only stand-in: two specks in the sky.
      return new Promise((r) =>
        setTimeout(
          () =>
            r([
              { kind: "heal", x: 0.21, y: 0.17, sourceX: 0.26, sourceY: 0.17, radius: 0.008, feather: 30, opacity: 100 },
              { kind: "heal", x: 0.81, y: 0.3, sourceX: 0.76, sourceY: 0.3, radius: 0.006, feather: 30, opacity: 100 },
            ]),
          120,
        ),
      );
    case "new_spot": {
      // Dev-only stand-in: the source a little to the right.
      const { kind, at, radius } = payload as { kind: "heal" | "clone"; at: [number, number]; radius: number };
      return { kind, x: at[0], y: at[1], sourceX: Math.min(1, at[0] + radius * 3), sourceY: at[1], radius, feather: 30, opacity: 100 };
    }
    case "auto_level":
      // Dev-only stand-in: a small tilt to correct.
      return new Promise((r) => setTimeout(() => r(-1.4), 60));
    case "measure_chromatic_aberration":
      // Dev-only stand-in: a little red and blue spread.
      return new Promise((r) => setTimeout(() => r({ red: [0.0004, 0.0001], blue: [-0.0003, 0] }), 250));
    case "paste_edits_to": {
      const { paths, source } = payload as { paths: string[]; source: Record<string, number> };
      for (const path of paths) mockEdits.set(path, source);
      return new Promise((r) => setTimeout(() => r({ applied: paths.map((path) => ({ path, edited: true })), failed: [] }), 250));
    }
    case "choose_export_folder":
      (mockSettings.export as Record<string, unknown>).folder = "/Users/me/Pictures/Exports/Lake District";
      return "/Users/me/Pictures/Exports/Lake District";
    case "start_export": {
      // Dev-only stand-in: a run that reports progress, then finishes.
      const { items } = (payload as { batch: { items: unknown[] } }).batch;
      const total = items.length;
      let done = 0;
      const tick = () => {
        if (done >= total) {
          void emit("export://queue", { type: "finished", exported: total, outputs: [], failed: [], folder: "/Users/me/Pictures/Exports/Lake District", cancelled: false });
          return;
        }
        void emit("export://queue", { type: "progress", done, total, current: `DSC_00${done}.NEF`, fraction: 0.5 });
        done++;
        setTimeout(tick, 1500);
      };
      setTimeout(tick, 100);
      return total;
    }
    case "cancel_exports":
      return null;
    case "save_edit": {
      const { path, recipe } = payload as { path: string; recipe: Record<string, number> };
      const edited = ["exposure", "contrast", "highlights", "shadows", "whites", "blacks", "dehaze", "temperature", "tint", "vibrance", "saturation", "texture", "clarity", "noiseReduction", "vignette", "grain"].some((k) => recipe[k] !== 0) || recipe.sharpening !== 40;
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
    case "choose_backup_copy_folder":
      mockBackups = { ...mockBackups, copy: { folder: "/Volumes/Backup Drive/Photos", connected: true, count: 1, latestAtMs: Date.now() } };
      return mockBackups;
    case "stop_backup_copies":
      mockBackups = { ...mockBackups, copy: null };
      return mockBackups;
    case "list_albums":
      return albumList();
    case "create_album": {
      const { name, paths } = payload as { name: string; paths: string[] };
      mockAlbums.set(nextAlbum, { name: name.trim(), paths: [...new Set(paths)] });
      return albumDto(nextAlbum++);
    }
    case "rename_album": {
      const { id, name } = payload as { id: number; name: string };
      mockAlbums.get(id)!.name = name.trim();
      return albumDto(id);
    }
    case "delete_album":
      mockAlbums.delete((payload as { id: number }).id);
      return null;
    case "add_to_album": {
      const { id, paths } = payload as { id: number; paths: string[] };
      const a = mockAlbums.get(id)!;
      a.paths = [...new Set([...a.paths, ...paths])];
      return albumDto(id);
    }
    case "remove_from_album": {
      const { id, paths } = payload as { id: number; paths: string[] };
      const a = mockAlbums.get(id)!;
      a.paths = a.paths.filter((p) => !paths.includes(p));
      return albumDto(id);
    }
    case "album_photos": {
      const { id } = payload as { id: number };
      const all = lastListing ?? mockListing("/Users/me/Photos/2026 Iceland");
      const members = new Set(mockAlbums.get(id)!.paths);
      return { album: albumDto(id), photos: all.photos.map((p, i) => ({ ...p, marks: marksOf(p.path, i) })).filter((p) => members.has(p.path)) };
    }
    case "search_library": {
      // Dev-only stand-in: file names containing every word.
      const { query } = payload as { query: string };
      const words = query.toLowerCase().split(/\s+/).filter(Boolean);
      const all = lastListing ?? mockListing("/Users/me/Photos/2026 Iceland");
      const photos = all.photos
        .map((p, i) => ({ ...p, marks: marksOf(p.path, i) }))
        .filter((p) => words.every((w) => p.path.toLowerCase().includes(w)));
      return new Promise((r) => setTimeout(() => r({ query, photos }), 40));
    }
    case "library_collection": {
      const kind = (payload as { kind: "picks" | "rated" | "rejected" | "recent" }).kind;
      const all = lastListing ?? mockListing("/Users/me/Photos/2026 Iceland");
      const photos = all.photos
        .map((p, i) => ({ ...p, marks: marksOf(p.path, i) }))
        .filter((p, i) => (kind === "recent" ? i < 64 : kind === "picks" ? p.marks.flag === "pick" : kind === "rejected" ? p.marks.flag === "reject" : p.marks.rating > 0));
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
