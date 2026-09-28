import { describe, expect, it } from "vitest";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { PreviewQuality } from "../../ipc/generated/PreviewQuality";
import { PreviewScheduler, type FrameInfo, type SchedulerDeps } from "./previewScheduler";

const recipe = (exposure: number): EditRecipe => ({ version: 1, exposure, contrast: 0, temperature: 0, saturation: 0 });

interface Call {
  recipe: EditRecipe;
  quality: PreviewQuality;
  resolve: (v: string) => void;
  reject: (e: unknown) => void;
}

function harness() {
  const calls: Call[] = [];
  const shown: Array<[string, FrameInfo]> = [];
  const frames: Array<() => void> = [];
  const timers = new Map<number, () => void>();
  let nextTimer = 1;
  const deps: SchedulerDeps<string> = {
    render: (r, q) => new Promise((resolve, reject) => calls.push({ recipe: r, quality: q, resolve, reject })),
    isCancellation: (e) => e === "cancelled",
    onFrame: (f, info) => shown.push([f, info]),
    onError: () => {},
    requestFrame: (cb) => frames.push(cb),
    setTimer: (cb) => {
      timers.set(nextTimer, cb);
      return nextTimer++;
    },
    clearTimer: (h) => timers.delete(h),
    now: () => 0,
  };
  const s = new PreviewScheduler(deps, 100);
  const tick = () => frames.splice(0).forEach((f) => f());
  const fireTimers = () => {
    const pending = [...timers.values()];
    timers.clear();
    pending.forEach((t) => t());
  };
  const settle = () => new Promise((r) => setTimeout(r, 0));
  return { s, calls, shown, tick, fireTimers, settle };
}

describe("PreviewScheduler", () => {
  it("coalesces updates within one animation frame", () => {
    const h = harness();
    h.s.request(recipe(0.1));
    h.s.request(recipe(0.2));
    h.s.request(recipe(0.3));
    h.tick();
    expect(h.calls.map((c) => [c.recipe.exposure, c.quality])).toEqual([[0.3, "interactive"]]);
  });

  it("sends a new request without waiting for the previous one", () => {
    const h = harness();
    h.s.request(recipe(1));
    h.tick();
    h.s.request(recipe(2));
    h.tick();
    expect(h.calls).toHaveLength(2);
  });

  it("drops stale responses and counts superseded ones", async () => {
    const h = harness();
    h.s.request(recipe(1));
    h.tick();
    h.s.request(recipe(2));
    h.tick();
    h.s.request(recipe(3));
    h.tick();
    h.calls[2]!.resolve("third");
    await h.settle();
    h.calls[0]!.reject("cancelled");
    h.calls[1]!.resolve("second (late)");
    await h.settle();
    expect(h.shown.map(([f]) => f)).toEqual(["third"]);
    expect(h.s.stats()).toMatchObject({ requested: 3, shown: 1, superseded: 1, stale: 1, errors: 0 });
  });

  it("refines with a detail render once changes settle", () => {
    const h = harness();
    h.s.request(recipe(1));
    h.tick();
    h.fireTimers();
    expect(h.calls.map((c) => c.quality)).toEqual(["interactive", "detail"]);
    expect(h.calls[1]!.recipe.exposure).toBe(1);
  });

  it("defers refinement while an interactive request is still pending", () => {
    const h = harness();
    h.s.request(recipe(1));
    h.fireTimers(); // timer fires before the animation frame flushed
    expect(h.calls).toHaveLength(0);
    h.tick();
    h.fireTimers();
    expect(h.calls.map((c) => c.quality)).toEqual(["interactive", "detail"]);
  });

  it("stops after dispose", async () => {
    const h = harness();
    h.s.request(recipe(1));
    h.tick();
    h.s.dispose();
    h.calls[0]!.resolve("late");
    await h.settle();
    h.s.request(recipe(2));
    h.tick();
    expect(h.shown).toHaveLength(0);
    expect(h.calls).toHaveLength(1);
  });
});
