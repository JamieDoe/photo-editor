import { describe, expect, it } from "vitest";
import { neutralRecipe } from "./recipe";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { PreviewQuality } from "../../ipc/generated/PreviewQuality";
import { PreviewScheduler, type FrameInfo, type SchedulerDeps } from "./previewScheduler";

const recipe = (exposure: number): EditRecipe => ({ ...neutralRecipe(1), exposure });

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

  it("waits for the render in flight, then sends only the newest change", async () => {
    const h = harness();
    h.s.request(recipe(1));
    h.tick();
    h.s.request(recipe(2));
    h.tick();
    h.s.request(recipe(3));
    h.tick();
    expect(h.calls).toHaveLength(1);
    h.calls[0]!.resolve("first");
    await h.settle();
    h.tick();
    expect(h.calls.map((c) => c.recipe.exposure)).toEqual([1, 3]);
    h.calls[1]!.resolve("third");
    await h.settle();
    expect(h.shown.map(([f]) => f)).toEqual(["first", "third"]);
  });

  it("keeps showing frames when renders are slower than frames", async () => {
    // Every frame brings a change, and every render outlasts several frames: each
    // render still completes and is shown (no starvation).
    const h = harness();
    for (let i = 1; i <= 12; i++) {
      h.s.request(recipe(i));
      h.tick();
      if (i % 3 === 0) {
        h.calls.at(-1)!.resolve(`frame ${i}`);
        await h.settle();
      }
    }
    expect(h.shown.length).toBeGreaterThanOrEqual(3);
    expect(h.s.stats().superseded).toBe(0);
  });

  it("drops stale responses and counts superseded ones", async () => {
    const h = harness();
    h.s.request(recipe(1));
    h.tick();
    h.fireTimers(); // settled: a detail render of recipe 1
    h.s.request(recipe(2)); // a new change while both are in flight
    h.tick();
    h.calls[0]!.resolve("interactive 1");
    await h.settle();
    h.tick();
    h.calls[2]!.resolve("interactive 2");
    await h.settle();
    h.calls[1]!.resolve("detail 1 (late)");
    await h.settle();
    expect(h.shown.map(([f]) => f)).toEqual(["interactive 1", "interactive 2"]);
    expect(h.s.stats()).toMatchObject({ requested: 3, shown: 2, stale: 1, errors: 0 });
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
