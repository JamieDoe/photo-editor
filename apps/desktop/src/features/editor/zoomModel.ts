import { clampCentre, fitScale, MAX_SCALE, panned, pointAt, steppedScale, zoomKeeping, type Size, type Zoom } from "./zoom";

/** What the viewer tells the zoom about itself: its viewport (CSS pixels), the photo's
 *  output (full-resolution pixels) and device pixels per CSS pixel. */
export interface ZoomGeometry {
  view: Size;
  full: Size;
  dpr: number;
}

/** A point of the viewport, in CSS pixels from its top left. */
export type ViewPoint = { x: number; y: number };

/** How the viewer changes the zoom (ADR 0070). */
export interface ZoomControl {
  /** Tells the zoom the viewer's geometry; null when there is no photo. */
  setGeometry(g: ZoomGeometry | null): void;
  /** Enlarges or reduces by `factor`, keeping the photo's point under `pointer`
   *  where it is (a pinch); at or below Fit, fits. */
  zoomBy(factor: number, pointer: ViewPoint): void;
  /** Moves the zoomed photo by `dx`, `dy` CSS pixels. */
  panBy(dx: number, dy: number): void;
  /** Animates to 100 % on the point under `pointer` (a click), or the view's centre. */
  zoomIn(pointer?: ViewPoint): void;
  /** Animates back to Fit. */
  fit(): void;
}

export interface ZoomDeps {
  onChange(zoom: Zoom | null): void;
  requestFrame(cb: (now: number) => void): number;
  cancelFrame(handle: number): void;
  now(): number;
}

/** How long a zoom animates (Z, a click, ⌘+ / ⌘−). */
export const ANIMATION_MS = 220;
const easeOut = (t: number) => 1 - (1 - t) ** 3;

/**
 * The editor's zoom (ADR 0070): Fit (null) or a zoom, from just above Fit to 800 %.
 * Pinches and pans change it at once; Z, clicks and ⌘+ / ⌘− animate to a level,
 * keeping a point of the photo where it is on screen. No React here: the clock and
 * frames are given, so it can be tested.
 */
export class ZoomModel implements ZoomControl {
  private zoom: Zoom | null = null;
  private geometry: ZoomGeometry | null = null;
  private frame: number | null = null;

  constructor(private readonly deps: ZoomDeps) {}

  get current(): Zoom | null {
    return this.zoom;
  }

  setGeometry(g: ZoomGeometry | null): void {
    this.geometry = g;
  }

  zoomBy(factor: number, pointer: ViewPoint): void {
    this.stop();
    const g = this.geometry;
    if (!g) return;
    const from = this.from(g);
    const scale = from.scale * factor;
    if (scale <= this.fitScale(g) * 1.001) return this.show(null);
    const at = pointAt(g.view, g.full, g.dpr, from, pointer);
    this.show(zoomKeeping(g.view, g.full, g.dpr, at, pointer, Math.min(MAX_SCALE, scale)));
  }

  panBy(dx: number, dy: number): void {
    this.stop();
    const g = this.geometry;
    if (g && this.zoom) this.show(panned(g.view, g.full, g.dpr, this.zoom, dx, dy));
  }

  zoomIn(pointer?: ViewPoint): void {
    const g = this.geometry;
    if (!g) return;
    // 100 %, or twice Fit when 100 % is hardly larger (a photo about the view's size
    // or smaller).
    const fit = this.fitScale(g);
    this.animateTo(fit * 1.25 < 1 ? 1 : Math.min(MAX_SCALE, fit * 2), pointer);
  }

  fit(): void {
    if (this.zoom === null && this.frame === null) return;
    this.animateTo(null);
  }

  /** Z and the toolbar button: Fit ↔ 100 %. */
  toggle(): void {
    if (this.zoom) this.fit();
    else this.zoomIn();
  }

  /** ⌘+ (+1) and ⌘− (−1): the next level, around the view's centre. */
  step(direction: 1 | -1): void {
    const g = this.geometry;
    if (!g) return;
    const fit = this.fitScale(g);
    const next = steppedScale(this.zoom?.scale ?? fit, direction, fit);
    if (next === null && this.zoom === null) return;
    this.animateTo(next);
  }

  /** Fits at once, with no animation (a tool that can't zoom opened). */
  reset(): void {
    this.stop();
    if (this.zoom !== null) this.show(null);
  }

  dispose(): void {
    this.stop();
  }

  private fitScale(g: ZoomGeometry): number {
    return fitScale(g.view, g.full, g.dpr);
  }

  /** The current zoom, or the one that looks like Fit, to start from. */
  private from(g: ZoomGeometry): Zoom {
    return this.zoom ?? { centre: { x: 0.5, y: 0.5 }, scale: this.fitScale(g) };
  }

  private show(z: Zoom | null): void {
    this.zoom = z;
    this.deps.onChange(z);
  }

  private stop(): void {
    if (this.frame !== null) this.deps.cancelFrame(this.frame);
    this.frame = null;
  }

  /** Animates to `scale` (null: Fit), keeping the photo's point under `pointer`, or
   *  the view's centre, in place. */
  private animateTo(scale: number | null, pointer?: ViewPoint): void {
    this.stop();
    const g = this.geometry;
    if (!g) return;
    const from = this.from(g);
    const target = scale ?? this.fitScale(g);
    const anchor = pointer ?? { x: g.view.width / 2, y: g.view.height / 2 };
    const at = pointAt(g.view, g.full, g.dpr, from, anchor);
    const start = this.deps.now();
    const tick = (now: number) => {
      const t = Math.min(1, (now - start) / ANIMATION_MS);
      if (t >= 1) {
        this.frame = null;
        this.show(scale === null ? null : zoomKeeping(g.view, g.full, g.dpr, at, anchor, target));
        return;
      }
      const e = easeOut(t);
      const s = from.scale * (target / from.scale) ** e;
      const keep = zoomKeeping(g.view, g.full, g.dpr, at, anchor, s).centre;
      // Towards Fit, the photo drifts back to the middle as it shrinks.
      const centre = scale === null ? { x: keep.x + (0.5 - keep.x) * e, y: keep.y + (0.5 - keep.y) * e } : keep;
      this.show({ centre: clampCentre(g.view, g.full, g.dpr, centre, s), scale: s });
      this.frame = this.deps.requestFrame(tick);
    };
    this.frame = this.deps.requestFrame(tick);
  }
}
