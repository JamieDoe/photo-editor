import { useState, type ReactNode } from "react";
import { ChevronIcon } from "../../components/icons";

/** Remembered per viewer (a convenience only: it starts open when storage is off). */
const key = (label: string) => `library.nav.${label}.open`;
function remembered(label: string): boolean {
  try {
    return localStorage.getItem(key(label)) !== "false";
  } catch {
    return true;
  }
}

/** A sidebar group (Folders, Albums) that folds away under its label; `action` (a +
 *  button) sits at the label's end. */
export function NavGroup({ label, action, children }: { label: string; action?: ReactNode; children: ReactNode }) {
  const [open, setOpen] = useState(() => remembered(label));
  const toggle = () => {
    setOpen(!open);
    try {
      localStorage.setItem(key(label), String(!open));
    } catch {
      /* not remembered */
    }
  };
  return (
    <section className="nav-section" aria-label={label}>
      <div className="nav-label">
        <button className="nav-toggle" aria-expanded={open} onClick={toggle}>
          <ChevronIcon size={12} />
          {label}
        </button>
        {action}
      </div>
      {open && children}
    </section>
  );
}
