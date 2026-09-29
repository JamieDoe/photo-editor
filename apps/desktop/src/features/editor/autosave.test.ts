import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { AUTOSAVE_DELAY_MS, Autosaver, type AutosaveDeps } from "./autosave";

const recipe = (exposure: number): EditRecipe => ({ version: 1, exposure, contrast: 0, temperature: 0, saturation: 0, look: "standard" });

function setup(save: AutosaveDeps["save"] = async (_p, r) => ({ edited: r.exposure !== 0 })) {
  const states: Array<[string, string, boolean | null]> = [];
  const saves: Array<[string, number]> = [];
  const saver = new Autosaver({
    save: async (p, r) => {
      saves.push([p, r.exposure]);
      return save(p, r);
    },
    setTimer: (cb, ms) => setTimeout(cb, ms) as unknown as number,
    clearTimer: (h) => clearTimeout(h),
    onState: (p, s, e) => states.push([p, s, e]),
  });
  return { saver, states, saves };
}

describe("Autosaver", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("saves once, after the changes stop", async () => {
    const { saver, saves, states } = setup();
    for (const e of [0.1, 0.2, 0.3]) {
      saver.schedule("/a", recipe(e));
      await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS / 2);
    }
    expect(saves).toEqual([]);
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS);
    expect(saves).toEqual([["/a", 0.3]]);
    expect(states).toEqual([
      ["/a", "saving", null],
      ["/a", "saved", true],
    ]);
  });

  it("writes the previous photo's edit at once when another photo takes over", async () => {
    const { saver, saves } = setup();
    saver.schedule("/a", recipe(1));
    saver.schedule("/b", recipe(2));
    await Promise.resolve();
    expect(saves).toEqual([["/a", 1]]);
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS);
    expect(saves).toEqual([
      ["/a", 1],
      ["/b", 2],
    ]);
  });

  it("keeps saves in order even when an earlier one is slow", async () => {
    const order: number[] = [];
    let release: () => void = () => {};
    const { saver } = setup(async (_p, r) => {
      if (r.exposure === 1) await new Promise<void>((res) => (release = res));
      order.push(r.exposure);
      return { edited: true };
    });
    saver.schedule("/a", recipe(1));
    void saver.flush();
    saver.schedule("/a", recipe(2));
    const done = saver.flush();
    await vi.advanceTimersByTimeAsync(0);
    release();
    await done;
    expect(order).toEqual([1, 2]);
  });

  it("reports failures and carries on", async () => {
    const { saver, states } = setup(async (_p, r) => {
      if (r.exposure === 1) throw new Error("disk full");
      return { edited: true };
    });
    saver.schedule("/a", recipe(1));
    await saver.flush();
    saver.schedule("/a", recipe(2));
    await saver.flush();
    expect(states.map((s) => s[1])).toEqual(["saving", "failed", "saving", "saved"]);
    expect(saver.hasPending()).toBe(false);
  });
});
