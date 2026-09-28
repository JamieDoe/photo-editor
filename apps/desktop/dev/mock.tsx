/**
 * Dev-only UI harness: runs the React app in a normal browser with Tauri IPC mocked,
 * for layout work without the Rust side. Not part of the production build (only
 * index.html is bundled). The placeholder frame is a flat test pattern, not a render.
 *
 *   npm run dev  ->  http://localhost:1420/dev/mock.html
 */
import { mockIPC } from "@tauri-apps/api/mocks";
import { createRoot } from "react-dom/client";
import { App } from "../src/App";
import "../src/styles.css";

const specs = [
  { key: "exposure", label: "Exposure", group: "Light", min: -5, max: 5, step: 0.01, default: 0 },
  { key: "contrast", label: "Contrast", group: "Light", min: -100, max: 100, step: 1, default: 0 },
  { key: "temperature", label: "Temperature", group: "Colour", min: -100, max: 100, step: 1, default: 0 },
  { key: "saturation", label: "Saturation", group: "Colour", min: -100, max: 100, step: 1, default: 0 },
];

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

mockIPC((cmd) => {
  switch (cmd) {
    case "engine_info":
      return { rendererVersion: 1, recipeVersion: 1, decoders: ["zune-jpeg", "libraw"], extensions: [], librawVersion: "mock", renderBackend: "cpu", jpegEncoder: "libjpeg-turbo", embeddedJpegDecoder: "libjpeg-turbo (DCT-scaled)", cpuThreads: 10, adjustments: specs };
    case "open_image_dialog":
    case "open_image_path":
      return { id: 1, fileName: "mock.nef", decoder: "libraw", cameraRaw: true, camera: "Mock Camera", fullWidth: 6000, fullHeight: 4000, levels: [[3000, 2000], [1500, 1000], [750, 500], [375, 250]], pyramidBytes: 0, identityMs: 0.5, decodeMs: 380, pyramidMs: 2, embeddedPreviewMs: 12 };
    case "render_preview":
      return placeholderFrame(600, 400);
    case "self_test_config":
      return null;
    default:
      return null;
  }
}, { shouldMockEvents: true });

createRoot(document.getElementById("root")!).render(<App />);
