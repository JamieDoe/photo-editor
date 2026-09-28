import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { PreviewQuality } from "../../ipc/generated/PreviewQuality";

/**
 * Decides *when* to ask Rust for a preview. No pixel work happens here.
 *
 * - Interactive requests are coalesced to at most one per animation frame.
 * - Each request is sent immediately; Rust cancels the superseded render, so the UI
 *   never waits for obsolete work to finish.
 * - Responses older than the newest displayed frame are dropped (stale).
 * - Once changes settle, one detail-quality render refines the preview.
 */
export interface SchedulerDeps<F> {
  render(recipe: EditRecipe, quality: PreviewQuality): Promise<F>;
  isCancellation(error: unknown): boolean;
  onFrame(frame: F, info: FrameInfo): void;
  onError(error: unknown): void;
  requestFrame(cb: () => void): void;
  setTimer(cb: () => void, ms: number): number;
  clearTimer(handle: number): void;
  now(): number;
}

export interface FrameInfo {
  seq: number;
  quality: PreviewQuality;
  /** Request sent -> frame received (IPC + queue + render + transfer). */
  roundTripMs: number;
}

export interface SchedulerStats {
  requested: number;
  shown: number;
  superseded: number;
  stale: number;
  errors: number;
}

export class PreviewScheduler<F> {
  private seq = 0;
  private lastShown = 0;
  private pending: EditRecipe | null = null;
  private frameRequested = false;
  private settleTimer: number | null = null;
  private disposed = false;
  private readonly counters: SchedulerStats = { requested: 0, shown: 0, superseded: 0, stale: 0, errors: 0 };

  constructor(
    private readonly deps: SchedulerDeps<F>,
    private readonly settleMs = 180,
  ) {}

  /** Request a preview for `recipe`: interactive now, detail once changes settle. */
  request(recipe: EditRecipe): void {
    if (this.disposed) return;
    this.pending = recipe;
    if (!this.frameRequested) {
      this.frameRequested = true;
      this.deps.requestFrame(() => this.flush());
    }
    this.armSettle(recipe);
  }

  stats(): SchedulerStats {
    return { ...this.counters };
  }

  dispose(): void {
    this.disposed = true;
    if (this.settleTimer !== null) this.deps.clearTimer(this.settleTimer);
  }

  private armSettle(recipe: EditRecipe): void {
    if (this.settleTimer !== null) this.deps.clearTimer(this.settleTimer);
    this.settleTimer = this.deps.setTimer(() => {
      this.settleTimer = null;
      if (this.disposed) return;
      // An interactive request is still waiting for its frame: refine after it.
      if (this.pending !== null) this.armSettle(recipe);
      else this.send(recipe, "detail");
    }, this.settleMs);
  }

  private flush(): void {
    this.frameRequested = false;
    const recipe = this.pending;
    this.pending = null;
    if (recipe !== null && !this.disposed) this.send(recipe, "interactive");
  }

  private send(recipe: EditRecipe, quality: PreviewQuality): void {
    const seq = ++this.seq;
    const start = this.deps.now();
    this.counters.requested++;
    this.deps.render(recipe, quality).then(
      (frame) => {
        if (this.disposed) return;
        if (seq < this.lastShown) {
          this.counters.stale++;
          return;
        }
        this.lastShown = seq;
        this.counters.shown++;
        this.deps.onFrame(frame, { seq, quality, roundTripMs: this.deps.now() - start });
      },
      (error: unknown) => {
        if (this.deps.isCancellation(error)) {
          this.counters.superseded++;
        } else if (!this.disposed) {
          this.counters.errors++;
          this.deps.onError(error);
        }
      },
    );
  }
}
