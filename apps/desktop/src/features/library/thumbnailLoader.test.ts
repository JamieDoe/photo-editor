import { describe, expect, it, vi } from "vitest";
import { requestThumbnail, type ThumbnailDeps } from "./thumbnailLoader";

function fakeDeps() {
  let resolve: (b: ArrayBuffer) => void = () => {};
  let reject: (e: unknown) => void = () => {};
  const deps: ThumbnailDeps = {
    fetch: vi.fn(
      () =>
        new Promise<ArrayBuffer>((res, rej) => {
          resolve = res;
          reject = rej;
        }),
    ),
    cancel: vi.fn(() => Promise.resolve()),
    createUrl: vi.fn(() => "blob:thumb"),
    revokeUrl: vi.fn(),
  };
  return { deps, resolve: (b: ArrayBuffer) => resolve(b), reject: (e: unknown) => reject(e) };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("requestThumbnail", () => {
  it("delivers an object URL and frees it on release", async () => {
    const { deps, resolve } = fakeDeps();
    const onReady = vi.fn();
    const release = requestThumbnail("/p/a.nef", onReady, deps);
    resolve(new ArrayBuffer(8));
    await flush();
    expect(onReady).toHaveBeenCalledWith("blob:thumb");
    release();
    expect(deps.revokeUrl).toHaveBeenCalledWith("blob:thumb");
    expect(deps.cancel).not.toHaveBeenCalled();
  });

  it("cancels a pending request and ignores its late result", async () => {
    const { deps, resolve } = fakeDeps();
    const onReady = vi.fn();
    const release = requestThumbnail("/p/a.nef", onReady, deps);
    release();
    release(); // idempotent
    expect(deps.cancel).toHaveBeenCalledTimes(1);
    expect(deps.cancel).toHaveBeenCalledWith("/p/a.nef");
    resolve(new ArrayBuffer(8));
    await flush();
    expect(onReady).not.toHaveBeenCalled();
    expect(deps.createUrl).not.toHaveBeenCalled();
  });

  it("keeps the placeholder when the thumbnail fails", async () => {
    const { deps, reject } = fakeDeps();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const onReady = vi.fn();
    requestThumbnail("/p/broken.nef", onReady, deps);
    reject({ kind: "decodeFailed", message: "x", reference: null });
    await flush();
    expect(onReady).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalledTimes(1);
    warn.mockRestore();
  });
});
