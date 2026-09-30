import { describe, expect, it } from "vitest";
import { decodeFrame, FRAME_HEADER_BYTES, HISTOGRAM_BINS } from "./frame";

function makeFrame(width: number, height: number, flags: number, renderMs: number): ArrayBuffer {
  const histogram = flags & 2 ? 4 * HISTOGRAM_BINS * 4 : 0;
  const buf = new ArrayBuffer(FRAME_HEADER_BYTES + histogram + width * height * 4);
  const v = new DataView(buf);
  v.setUint32(0, width, true);
  v.setUint32(4, height, true);
  v.setUint32(8, 2, true);
  v.setUint32(12, flags, true);
  v.setFloat32(16, renderMs, true);
  v.setUint32(20, 6000, true);
  v.setUint32(24, 4000, true);
  for (let i = 0; i < histogram / 4; i++) v.setUint32(FRAME_HEADER_BYTES + i * 4, i, true);
  new Uint8Array(buf, FRAME_HEADER_BYTES + histogram).fill(7);
  return buf;
}

describe("decodeFrame", () => {
  it("reads the header and exposes pixels without copying", () => {
    const buf = makeFrame(3, 2, 1, 12.5);
    const f = decodeFrame(buf);
    expect([f.width, f.height, f.level, f.cacheHit, f.renderMs]).toEqual([3, 2, 2, true, 12.5]);
    expect([f.fullWidth, f.fullHeight]).toEqual([6000, 4000]);
    expect(decodeFrame(makeFrame(1, 1, 0, 0)).cacheHit).toBe(false);
    expect(f.pixels.length).toBe(24);
    expect(f.pixels.buffer).toBe(buf);
    expect(f.pixels[0]).toBe(7);
  });

  it("reads the histogram when flagged, before the pixels", () => {
    const f = decodeFrame(makeFrame(3, 2, 2, 1));
    expect(f.histogram).not.toBeNull();
    // Counts were written as their index: red 0..255, green 256.., luma 768..
    expect(f.histogram!.red[5]).toBe(5);
    expect(f.histogram!.green[0]).toBe(256);
    expect(f.histogram!.luma[255]).toBe(1023);
    expect(f.pixels.length).toBe(24);
    expect(f.pixels[0]).toBe(7);
    expect(decodeFrame(makeFrame(3, 2, 0, 1)).histogram).toBeNull();
  });

  it("rejects truncated or inconsistent frames", () => {
    expect(() => decodeFrame(new ArrayBuffer(4))).toThrow();
    expect(() => decodeFrame(makeFrame(3, 2, 0, 0).slice(0, 30))).toThrow();
  });
});
