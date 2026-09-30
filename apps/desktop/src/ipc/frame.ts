/**
 * Binary preview frame decoding. Layout is defined in `src-tauri/src/ipc.rs`
 * (FRAME_HEADER_BYTES): seven little-endian 32-bit header fields, the histogram if
 * flagged, then RGBA8 pixels. Decoding creates views over the pixels; it never copies
 * or processes them.
 */
export const FRAME_HEADER_BYTES = 28;
const FLAG_CACHE_HIT = 1;
const FLAG_HISTOGRAM = 2;
export const HISTOGRAM_BINS = 256;
const HISTOGRAM_BYTES = 4 * HISTOGRAM_BINS * 4;

/** Pixel counts per 8-bit value of the rendered frame (ADR 0036). */
export interface FrameHistogram {
  red: Uint32Array;
  green: Uint32Array;
  blue: Uint32Array;
  luma: Uint32Array;
}

export interface PreviewFrame {
  width: number;
  height: number;
  level: number;
  cacheHit: boolean;
  /** Rust-side render time (0 for cache hits). */
  renderMs: number;
  /** The recipe's output size at the photo's full resolution (after crop): the
   *  picture's exact shape, which preview levels only approximate. */
  fullWidth: number;
  fullHeight: number;
  /** Viewer frames carry their histogram; null otherwise. */
  histogram: FrameHistogram | null;
  pixels: Uint8ClampedArray<ArrayBuffer>;
}

export function decodeFrame(buffer: ArrayBuffer): PreviewFrame {
  if (buffer.byteLength < FRAME_HEADER_BYTES) {
    throw new Error(`preview frame too short: ${buffer.byteLength} bytes`);
  }
  const view = new DataView(buffer);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  const level = view.getUint32(8, true);
  const flags = view.getUint32(12, true);
  const renderMs = view.getFloat32(16, true);
  const fullWidth = view.getUint32(20, true);
  const fullHeight = view.getUint32(24, true);
  const hasHistogram = (flags & FLAG_HISTOGRAM) !== 0;
  const pixelOffset = FRAME_HEADER_BYTES + (hasHistogram ? HISTOGRAM_BYTES : 0);
  const expected = pixelOffset + width * height * 4;
  if (buffer.byteLength !== expected) {
    throw new Error(`preview frame size ${buffer.byteLength} != expected ${expected}`);
  }
  return {
    width,
    height,
    level,
    cacheHit: (flags & FLAG_CACHE_HIT) !== 0,
    renderMs,
    fullWidth,
    fullHeight,
    histogram: hasHistogram ? readHistogram(view, FRAME_HEADER_BYTES) : null,
    pixels: new Uint8ClampedArray(buffer, pixelOffset, width * height * 4),
  };
}

/** Four planes of 256 little-endian u32 counts (4 KB; copied, whatever the platform's
 *  byte order). */
function readHistogram(view: DataView, offset: number): FrameHistogram {
  const plane = (k: number) => {
    const out = new Uint32Array(HISTOGRAM_BINS);
    for (let i = 0; i < HISTOGRAM_BINS; i++) out[i] = view.getUint32(offset + (k * HISTOGRAM_BINS + i) * 4, true);
    return out;
  };
  return { red: plane(0), green: plane(1), blue: plane(2), luma: plane(3) };
}
