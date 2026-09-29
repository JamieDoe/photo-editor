/** Whether a key event's target takes typed text (so single-key shortcuts must not fire). */
export function isTextEntry(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (target instanceof HTMLInputElement) {
    return !["range", "radio", "checkbox", "button", "submit"].includes(target.type);
  }
  return false;
}

/** Plain key presses only: shortcuts ignore anything with Cmd, Ctrl or Alt held. */
export const hasCommandModifier = (e: KeyboardEvent) => e.metaKey || e.ctrlKey || e.altKey;
