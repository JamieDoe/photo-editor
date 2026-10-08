import type { WatermarkDto } from "../../ipc/generated/WatermarkDto";
import type { WatermarkSettings } from "../../ipc/generated/WatermarkSettings";

/** The drawing's text size: large enough to stay crisp when Rust scales it down onto a
 *  photo (ADR 0069). */
const FONT_PX = 160;
/** Room around the text for its shadow. */
const PAD_PX = 32;

/** The watermark to send with an export, or null when it is off or has no text. */
export async function watermarkFor(w: WatermarkSettings): Promise<WatermarkDto | null> {
  const text = w.text.trim();
  if (!w.enabled || text.length === 0) return null;
  const png = await drawText(text);
  return png ? { png: Array.from(png), position: w.position, size: w.size } : null;
}

/** `text` in the app's typeface, white with a soft dark shadow so it reads on light and
 *  dark photos alike, as an RGBA PNG. Null if the canvas can't be made. */
async function drawText(text: string): Promise<Uint8Array | null> {
  // The UI's own face, once it has loaded.
  await document.fonts?.ready;
  const font = `600 ${FONT_PX}px ${getComputedStyle(document.documentElement).getPropertyValue("--font") || "sans-serif"}`;
  const measure = document.createElement("canvas").getContext("2d");
  if (!measure) return null;
  measure.font = font;
  const m = measure.measureText(text);
  const ascent = m.actualBoundingBoxAscent || FONT_PX * 0.75;
  const descent = m.actualBoundingBoxDescent || FONT_PX * 0.2;
  const canvas = document.createElement("canvas");
  canvas.width = Math.ceil(m.width + 2 * PAD_PX);
  canvas.height = Math.ceil(ascent + descent + 2 * PAD_PX);
  const g = canvas.getContext("2d");
  if (!g) return null;
  g.font = font;
  g.textBaseline = "alphabetic";
  g.shadowColor = "rgba(0, 0, 0, 0.55)";
  g.shadowBlur = 14;
  g.shadowOffsetY = 3;
  g.fillStyle = "#ffffff";
  g.fillText(text, PAD_PX, PAD_PX + ascent);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  return blob ? new Uint8Array(await blob.arrayBuffer()) : null;
}
