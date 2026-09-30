import { useCallback, useEffect, useRef, useState } from "react";
import { CheckIcon } from "./icons";

/** How long a confirmation stays up. */
const TOAST_MS = 2200;

/** A short confirmation ("Edits copied"), as the design shows them: one at a time, a
 *  newer one replacing the last. */
export function useToast() {
  const [toast, setToast] = useState<{ id: number; message: string } | null>(null);
  const timer = useRef<number | null>(null);
  const notify = useCallback((message: string) => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    setToast((t) => ({ id: (t?.id ?? 0) + 1, message }));
    timer.current = window.setTimeout(() => setToast(null), TOAST_MS);
  }, []);
  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );
  return { toast, notify };
}

export function Toast({ toast }: { toast: { id: number; message: string } | null }) {
  if (!toast) return null;
  return (
    <div key={toast.id} className="toast" role="status">
      <CheckIcon size={15} strokeWidth={2} />
      {toast.message}
    </div>
  );
}
