import type { EditRecipe } from "../../ipc/generated/EditRecipe";

export type SaveState = "saved" | "saving" | "failed";

export interface AutosaveDeps {
  save: (path: string, recipe: EditRecipe) => Promise<{ edited: boolean }>;
  setTimer: (cb: () => void, ms: number) => number;
  clearTimer: (handle: number) => void;
  /** Called when a photo's save starts, succeeds (with whether it is now edited) or fails. */
  onState: (path: string, state: SaveState, edited: boolean | null, error?: unknown) => void;
}

/** Quiet time after the last change before saving: a slider drag is one save. */
export const AUTOSAVE_DELAY_MS = 400;

/**
 * Saves the open photo's edit a moment after it stops changing (ADR 0019). Saves run
 * one after another, so an older recipe can never land after a newer one, and a
 * pending save is written immediately when another photo takes over.
 */
export class Autosaver {
  private pending: { path: string; recipe: EditRecipe } | null = null;
  private timer: number | null = null;
  private chain: Promise<void> = Promise.resolve();

  constructor(private readonly deps: AutosaveDeps) {}

  /** The recipe of `path` changed. */
  schedule(path: string, recipe: EditRecipe): void {
    if (this.pending && this.pending.path !== path) this.flush();
    this.pending = { path, recipe };
    if (this.timer !== null) this.deps.clearTimer(this.timer);
    this.timer = this.deps.setTimer(() => this.flush(), AUTOSAVE_DELAY_MS);
  }

  /** Writes any pending save now. Resolves once every save so far has finished. */
  flush(): Promise<void> {
    if (this.timer !== null) {
      this.deps.clearTimer(this.timer);
      this.timer = null;
    }
    const job = this.pending;
    this.pending = null;
    if (job) {
      this.deps.onState(job.path, "saving", null);
      this.chain = this.chain.then(async () => {
        try {
          const { edited } = await this.deps.save(job.path, job.recipe);
          this.deps.onState(job.path, "saved", edited);
        } catch (e) {
          this.deps.onState(job.path, "failed", null, e);
        }
      });
    }
    return this.chain;
  }

  hasPending(): boolean {
    return this.pending !== null;
  }
}
