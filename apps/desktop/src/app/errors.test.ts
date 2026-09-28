import { describe, expect, it, vi } from "vitest";

vi.mock("../ipc/client", () => ({
  isIpcError: (e: unknown) => typeof e === "object" && e !== null && "kind" in e && "message" in e,
  isCancellation: () => false,
  reportClientError: vi.fn(async () => "E-TEST-1"),
}));

import * as ipc from "../ipc/client";
import { formatErrorDetails, report, toAppError } from "./errors";

describe("toAppError", () => {
  it("keeps Rust's user-facing message and reference", async () => {
    const e = { kind: "decodeFailed", message: "This RAW file could not be decoded.", reference: "E-X-2" };
    expect(await toAppError(e)).toEqual({ message: "This RAW file could not be decoded.", reference: "E-X-2" });
    expect(ipc.reportClientError).not.toHaveBeenCalled();
  });

  it("logs unexpected errors and shows a generic message", async () => {
    const result = await toAppError(new TypeError("x is undefined"));
    expect(result).toEqual({ message: "Something went wrong. Please try again.", reference: "E-TEST-1" });
    expect(ipc.reportClientError).toHaveBeenCalledWith(
      expect.objectContaining({ source: "unhandledRejection", message: "TypeError: x is undefined" }),
    );
  });
});

describe("report", () => {
  it("never throws, even if logging fails", async () => {
    vi.mocked(ipc.reportClientError).mockRejectedValueOnce(new Error("ipc down"));
    await expect(report("render", "boom")).resolves.toBeNull();
  });
});

describe("formatErrorDetails", () => {
  it("includes message, reference, versions and log location", () => {
    const text = formatErrorDetails(
      { message: "Export failed.", reference: "E-AB12-7" },
      {
        appVersion: "0.0.1",
        os: "macos",
        arch: "aarch64",
        cpuThreads: 10,
        rendererVersion: 1,
        librawVersion: "0.22.2",
        jpegEncoder: "libjpeg-turbo",
        embeddedJpegDecoder: "libjpeg-turbo (DCT-scaled)",
        logDir: "/logs",
      },
      new Date("2026-09-28T10:00:00Z"),
    );
    expect(text).toBe(
      [
        "Message: Export failed.",
        "Reference: E-AB12-7",
        "Time: 2026-09-28T10:00:00.000Z",
        "App: 0.0.1 · macos aarch64 · 10 threads",
        "Renderer: v1 · LibRaw 0.22.2 · libjpeg-turbo · libjpeg-turbo (DCT-scaled)",
        "Logs: /logs",
      ].join("\n"),
    );
  });

  it("works without diagnostics", () => {
    expect(formatErrorDetails({ message: "m", reference: null }, null, new Date(0))).toBe(
      "Message: m\nReference: none\nTime: 1970-01-01T00:00:00.000Z",
    );
  });
});
