import { useEffect, useRef, useState } from "react";
import { isCancellation } from "../../ipc/client";
import type { PreviewFrame } from "../../ipc/frame";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { beforeRecipe } from "./recipe";
import type { Editor } from "./useEditor";

/** Where the divider starts, and how far it can go (percent of the width). */
const START_SPLIT = 50;
const MIN_SPLIT = 2;
const MAX_SPLIT = 98;

/** Before/after (ADR 0045): whether it is on, and where the divider is. */
export function useCompare() {
  const [open, setOpen] = useState(false);
  const [split, setSplit] = useState(START_SPLIT);
  return {
    open,
    split,
    toggle: () => setOpen((o) => !o),
    close: () => setOpen(false),
    setSplit: (v: number) => setSplit(Math.min(MAX_SPLIT, Math.max(MIN_SPLIT, v))),
  };
}

export type Compare = ReturnType<typeof useCompare>;

/**
 * The comparison over the photo, as in the design: the photo before editing left of
 * the divider, the edit (the viewer's own canvas, beneath) right of it. The before
 * image is rendered once for the photo, its geometry and the box's size, in its own
 * render slot; moving the divider only changes a clip, and editing renders only the
 * edit.
 */
export function CompareOverlay({
  compare,
  editor,
  recipe,
  specs,
}: {
  compare: Compare;
  editor: Editor;
  recipe: EditRecipe;
  specs: AdjustmentSpec[];
}) {
  const boxRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [edge, setEdge] = useState(0);
  /** The latest before image, and the photo and recipe it shows. */
  const [rendered, setRendered] = useState<{ frame: PreviewFrame; shows: string } | null>(null);
  const [failed, setFailed] = useState(false);

  // The box's long edge in device pixels, in steps, so small resizes keep the frame.
  useEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      setEdge(Math.ceil((Math.max(width, height) * window.devicePixelRatio) / 64) * 64);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const imageId = editor.image?.id ?? null;
  const target = beforeRecipe(recipe, specs);
  const shows = `${imageId}:${JSON.stringify(target)}`;
  const key = `${shows}:${edge}`;
  // Only a render of this photo as it is cropped now lines up; a size change can keep
  // showing the last one until the next arrives.
  const before = rendered?.shows === shows ? rendered.frame : null;
  const { renderCompare } = editor;
  useEffect(() => {
    if (imageId === null || edge === 0) return;
    let live = true;
    setFailed(false);
    renderCompare(target, edge).then(
      (frame) => live && setRendered({ frame, shows }),
      (e: unknown) => {
        if (live && !isCancellation(e)) setFailed(true);
      },
    );
    return () => {
      live = false;
    };
    // `key` stands for the photo, the box and the before recipe.
  }, [key, renderCompare]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !before) return;
    if (canvas.width !== before.width || canvas.height !== before.height) {
      canvas.width = before.width;
      canvas.height = before.height;
    }
    canvas.getContext("2d")?.putImageData(new ImageData(before.pixels, before.width, before.height), 0, 0);
    canvas.dataset.drawn = key;
  }, [before, key]);

  const at = `${compare.split}%`;
  return (
    <div className="compare-overlay" ref={boxRef}>
      <canvas
        ref={canvasRef}
        className="compare-before"
        style={{ clipPath: `inset(0 ${100 - compare.split}% 0 0)`, visibility: before ? "visible" : "hidden" }}
        aria-hidden="true"
      />
      <div className="compare-line" style={{ left: at }} />
      <div className="compare-handle" style={{ left: at }} aria-hidden="true">
        <svg width="16" height="16" viewBox="0 0 16 16">
          <path d="M6 4.5 2.5 8 6 11.5M10 4.5l3.5 3.5-3.5 3.5" />
        </svg>
      </div>
      <span className="compare-label before">{before || failed ? "Before" : "Before…"}</span>
      <span className="compare-label after">After</span>
      <input
        className="compare-splitter"
        type="range"
        min={MIN_SPLIT}
        max={MAX_SPLIT}
        step={0.5}
        value={compare.split}
        onChange={(e) => compare.setSplit(Number(e.target.value))}
        aria-label="Drag to compare before and after"
      />
    </div>
  );
}
