/**
 * Binary preview frame decoding. Layout is defined in `src-tauri/src/ipc.rs`
 * (FRAME_HEADER_BYTES): seven little-endian 32-bit header fields, then RGBA8 pixels.
 * Decoding creates views over the buffer; it never copies or processes pixels.
 */
export const FRAME_HEADER_BYTES = 28;

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
  const expected = FRAME_HEADER_BYTES + width * height * 4;
  if (buffer.byteLength !== expected) {
    throw new Error(`preview frame size ${buffer.byteLength} != expected ${expected}`);
  }
  return {
    width,
    height,
    level,
    cacheHit: (flags & 1) !== 0,
    renderMs,
    fullWidth,
    fullHeight,
    pixels: new Uint8ClampedArray(buffer, FRAME_HEADER_BYTES, width * height * 4),
  };
}
