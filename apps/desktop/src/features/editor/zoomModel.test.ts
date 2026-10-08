import { describe, expect, it } from "vitest";
import { pointAt, zoomLayout, type Zoom } from "./zoom";
import { ANIMATION_MS, ZoomModel } from "./zoomModel";

const geometry = { view: { width: 1000, height: 600 }, full: { width: 6000, height: 4000 }, dpr: 1 };
const FIT = 0.15; // 600 / 4000

/** A model on a hand-driven clock: `run` plays the frames asked for. */
function setup() {
  const shown: Array<Zoom | null> = [];
  let now = 0;
  let pending: Array<(t: number) => void> = [];
  const model = new ZoomModel({
    onChange: (z) => shown.push(z),
    requestFrame: (cb) => pending.push(cb),
    cancelFrame: () => {
      pending = [];
    },
    now: () => now,
  });
  model.setGeometry(geometry);
  const run = (ms = ANIMATION_MS + 50, step = 16) => {
    for (let t = 0; t <= ms && pending.length > 0; t += step) {
      now += step;
      const frames = pending;
      pending = [];
      frames.forEach((f) => f(now));
    }
  };
  return { model, shown, run };
}

describe("ZoomModel", () => {
  it("animates from Fit to 100 % through the levels between", () => {
    const { model, shown, run } = setup();
    model.toggle();
    run();
    const scales = shown.map((z) => z?.scale ?? 0);
    expect(scales.length).toBeGreaterThan(5);
    expect(scales.at(-1)).toBe(1);
    expect(scales.slice(0, -1).every((s) => s > FIT && s < 1)).toBe(true);
    expect(scales.every((s, i) => i === 0 || s >= scales[i - 1]!)).toBe(true);
  });

  it("animates back to Fit, and fitting at Fit does nothing", () => {
    const { model, shown, run } = setup();
    model.fit();
    run();
    expect(shown).toEqual([]);
    model.toggle();
    run();
    model.toggle();
    run();
    expect(shown.at(-1)).toBeNull();
    expect(model.current).toBeNull();
  });

  it("zooms in on the spot clicked, which stays under the pointer", () => {
    const { model, shown, run } = setup();
    const pointer = { x: 250, y: 400 };
    const at = pointAt(geometry.view, geometry.full, 1, { centre: { x: 0.5, y: 0.5 }, scale: FIT }, pointer);
    model.zoomIn(pointer);
    run();
    const last = shown.at(-1)!;
    const z = zoomLayout(geometry.view, geometry.full, 1, last);
    expect(z.left + at.x * z.width).toBeCloseTo(pointer.x, 0);
    expect(z.top + at.y * z.height).toBeCloseTo(pointer.y, 0);
  });

  it("pinches smoothly, down to Fit and up to 800 %", () => {
    const { model, shown } = setup();
    const pointer = { x: 500, y: 300 };
    model.zoomBy(1.1, pointer);
    model.zoomBy(1.1, pointer);
    expect(shown.map((z) => z?.scale)).toEqual([FIT * 1.1, FIT * 1.1 * 1.1].map((s) => expect.closeTo(s, 6)));
    for (let i = 0; i < 100; i++) model.zoomBy(1.2, pointer);
    expect(model.current?.scale).toBe(8);
    for (let i = 0; i < 100; i++) model.zoomBy(0.8, pointer);
    expect(model.current).toBeNull();
  });

  it("steps through levels with ⌘+ and ⌘−", () => {
    const { model, run } = setup();
    model.step(1);
    run();
    expect(model.current?.scale).toBe(1 / 8 > FIT ? 1 / 8 : 1 / 4);
    model.step(1);
    run();
    expect(model.current?.scale).toBe(1 / 3);
    model.step(-1);
    run();
    model.step(-1);
    run();
    expect(model.current).toBeNull();
  });

  it("stops an animation when the photo is panned", () => {
    const { model, run } = setup();
    model.toggle();
    run(40);
    const mid = model.current!.scale;
    model.panBy(10, 0);
    run();
    expect(model.current!.scale).toBe(mid);
  });
});
