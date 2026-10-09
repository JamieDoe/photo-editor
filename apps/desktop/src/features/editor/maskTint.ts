import type { Combine } from "../../ipc/generated/Combine";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { Mask } from "../../ipc/generated/Mask";
import type { MaskShape } from "../../ipc/generated/MaskShape";
import { BrushTint, canvas2d, fit, type TintView } from "./brushTint";
import { shapesOf, toShown } from "./masks";

/**
 * A mask's tint on a canvas (ADRs 0040–0043): where it covers the photo, in the accent
 * colour at its density. Each shape's coverage is drawn as alpha, as the renderer
 * computes it, and the shapes are combined with the canvas's compositing, which is the
 * renderer's arithmetic on coverage:
 * - add, "source-over": `p + c·(1 - p)`;
 * - subtract, "destination-out": `c·(1 - p)`;
 * - intersect, "destination-in": `c·p`.
 */
const COMPOSITE: Record<Combine, GlobalCompositeOperation> = {
  add: "source-over",
  subtract: "destination-out",
  intersect: "destination-in",
};

/** Stops of `1 - smoothstep` over `from`..1 of a gradient, as alpha. */
function fadeStops(g: CanvasGradient, from: number): void {
  const steps = 12;
  for (let k = 0; k <= steps; k++) {
    const t = k / steps;
    g.addColorStop(from + t * (1 - from), `rgba(0,0,0,${1 - t * t * (3 - 2 * t)})`);
  }
}

/** A linear or radial gradient's coverage filled over `ctx`'s canvas. */
function fillGradient(ctx: CanvasRenderingContext2D, shape: Extract<MaskShape, { kind: "linear" | "radial" }>, view: TintView): void {
  const { width: W, height: H, crop } = view;
  const px = (p: [number, number]) => {
    const q = toShown(p, crop);
    return [q[0] * W, q[1] * H] as const;
  };
  ctx.save();
  if (shape.kind === "linear") {
    const [s, e] = [px(shape.start), px(shape.end)];
    const g = ctx.createLinearGradient(s[0], s[1], e[0], e[1]);
    fadeStops(g, 0);
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, W, H);
  } else {
    // Radii are fractions of the frame's diagonal.
    const diagonal = Math.hypot(W / crop.w, H / crop.h);
    const [a, b] = shape.radius.map((r) => Math.max(r * diagonal, 0.01)) as [number, number];
    const c = px(shape.centre);
    ctx.translate(c[0], c[1]);
    ctx.rotate((shape.angle * Math.PI) / 180);
    ctx.scale(a, b);
    const g = ctx.createRadialGradient(0, 0, 0, 0, 0, 1);
    fadeStops(g, Math.min(1 - shape.feather / 100, 0.999));
    ctx.fillStyle = g;
    // The whole canvas, in the ellipse's units.
    const reach = (W + H) / Math.min(a, b) + 2;
    ctx.fillRect(-reach, -reach, 2 * reach, 2 * reach);
  }
  ctx.restore();
}

/** A generated mask's coverage (ADR 0074) as a canvas's alpha, over `crop` of the
 *  frame: made by the engine, which maps it as the renderer does. */
export interface GeneratedView {
  canvas: HTMLCanvasElement;
  crop: CropRect;
}

/** `width` x `height` coverage bytes as a canvas's alpha. */
export function coverageCanvas(bytes: Uint8Array, width: number, height: number): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const image = new ImageData(width, height);
  for (let i = 0; i < width * height; i++) image.data[i * 4 + 3] = bytes[i] ?? 0;
  canvas2d(canvas).putImageData(image, 0, 0);
  return canvas;
}

export class MaskTint {
  /** Brush shapes' layers, by the shape's place in the mask. */
  private readonly brushes: BrushTint[] = [];
  private readonly combined = document.createElement("canvas");
  private readonly layer = document.createElement("canvas");

  /** Draws `mask`'s coverage on `target` in `colour`; its generated shapes from
   *  `generated`, by name (a shape not there yet covers nothing). */
  draw(
    target: HTMLCanvasElement,
    mask: Mask,
    view: TintView,
    colour: string,
    generated: ReadonlyMap<string, GeneratedView> = new Map(),
  ): void {
    const { width, height } = view;
    for (const c of [target, this.combined, this.layer]) fit(c, width, height);
    const combined = canvas2d(this.combined);
    combined.clearRect(0, 0, width, height);
    shapesOf(mask).forEach(({ shape, mode }, i) => {
      combined.globalCompositeOperation = mode ? COMPOSITE[mode] : "source-over";
      combined.drawImage(this.shapeLayer(shape, i, view, generated), 0, 0);
    });
    combined.globalCompositeOperation = "source-over";

    const out = canvas2d(target);
    out.save();
    out.clearRect(0, 0, width, height);
    if (mask.invert) {
      out.fillRect(0, 0, width, height);
      out.globalCompositeOperation = "destination-out";
    }
    out.drawImage(this.combined, 0, 0);
    // Coloured where covered, at the mask's density.
    out.globalCompositeOperation = "source-in";
    out.globalAlpha = (mask.density ?? 100) / 100;
    out.fillStyle = colour;
    out.fillRect(0, 0, width, height);
    out.restore();
  }

  /** Shape `i`'s coverage, as alpha. */
  private shapeLayer(
    shape: MaskShape,
    i: number,
    view: TintView,
    generated: ReadonlyMap<string, GeneratedView>,
  ): HTMLCanvasElement {
    if (shape.kind === "brush") return (this.brushes[i] ??= new BrushTint()).coverage(shape.strokes, view);
    const layer = canvas2d(this.layer);
    layer.clearRect(0, 0, view.width, view.height);
    if (shape.kind !== "generated") {
      fillGradient(layer, shape, view);
      return this.layer;
    }
    const made = generated.get(shape.mask);
    if (made) {
      // The part of the made view this view shows, scaled smoothly.
      const { canvas, crop } = made;
      const v = view.crop;
      const sx = ((v.x - crop.x) / crop.w) * canvas.width;
      const sy = ((v.y - crop.y) / crop.h) * canvas.height;
      const sw = (v.w / crop.w) * canvas.width;
      const sh = (v.h / crop.h) * canvas.height;
      layer.imageSmoothingQuality = "high";
      layer.drawImage(canvas, sx, sy, sw, sh, 0, 0, view.width, view.height);
    }
    return this.layer;
  }
}
