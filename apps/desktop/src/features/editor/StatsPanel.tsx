import type { Editor } from "./useEditor";

const fmt = (ms: number) => `${ms.toFixed(1)} ms`;

/** Developer diagnostics for Phase 0: timings, cache and supersession counters. */
export function StatsPanel({ editor }: { editor: Editor }) {
  const { info, image, displayed, stats, exportState } = editor;
  const rows: Array<[string, string]> = [];
  if (info) {
    rows.push(["Renderer", `v${info.rendererVersion} · ${info.renderBackend} · ${info.cpuThreads} threads`]);
    rows.push(["Decoders", `${info.decoders.join(", ")}${info.librawVersion ? ` (LibRaw ${info.librawVersion})` : ""}`]);
    rows.push(["JPEG", `export: ${info.jpegEncoder} · embedded: ${info.embeddedJpegDecoder}`]);
  }
  if (image) {
    rows.push(["File", `${image.fileName}${image.camera ? ` · ${image.camera}` : ""}`]);
    rows.push(["Full size", `${image.fullWidth}×${image.fullHeight}`]);
    rows.push(["Pyramid", image.levels.map(([w, h]) => `${w}×${h}`).join(" · ")]);
    rows.push(["Open", `decode ${fmt(image.decodeMs)} · pyramid ${fmt(image.pyramidMs)} · id ${fmt(image.identityMs)}`]);
  }
  if (image?.embeddedPreviewMs != null) {
    rows.push(["Embedded", `extracted in ${fmt(image.embeddedPreviewMs)}`]);
  }
  if (displayed?.source === "embedded") {
    const f = displayed.frame;
    rows.push(["Frame", `${f.width}×${f.height} · embedded camera preview (decoding…)`]);
    rows.push(["Shown after", fmt(displayed.sinceOpenMs)]);
  } else if (displayed) {
    const f = displayed.frame;
    rows.push(["Frame", `${f.width}×${f.height} · L${f.level} · ${displayed.info.quality}${f.cacheHit ? " · cache hit" : ""}`]);
    rows.push(["Render (Rust)", fmt(f.renderMs)]);
    rows.push(["Round trip", fmt(displayed.info.roundTripMs)]);
  }
  if (stats) {
    rows.push(["Requests", `${stats.requested} sent · ${stats.shown} shown · ${stats.superseded} superseded · ${stats.stale} stale`]);
  }
  if (exportState) {
    const e = exportState.last;
    const status =
      e === null ? "queued" : e.type === "progress" ? `${e.stage} ${(e.fraction * 100).toFixed(0)}%` : e.type === "finished" ? `done in ${fmt(e.totalMs)} (${e.width}×${e.height})` : "failed";
    rows.push(["Export", status]);
  }
  return (
    <dl className="stats">
      {rows.map(([k, v]) => (
        <div key={k}>
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}
