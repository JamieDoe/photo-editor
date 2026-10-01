import { useState, type ReactNode } from "react";
import { ChevronIcon } from "../../components/icons";

interface Props {
  title: string;
  icon: ReactNode;
  /** Shows the accent dot: something in this section differs from its default. */
  edited?: boolean;
  /** A count beside the title (the design's number of masks). */
  count?: string;
  defaultOpen?: boolean;
  /** Controlled open state, for a section whose being open changes the photo (the
   *  Retouch section puts the photo in retouch mode). */
  open?: boolean;
  onToggle?: (open: boolean) => void;
  children: ReactNode;
}

/** A collapsible section of the right-hand panel. */
export function PanelSection({ title, icon, edited = false, count, defaultOpen = true, open: controlled, onToggle, children }: Props) {
  const [own, setOwn] = useState(defaultOpen);
  const open = controlled ?? own;
  const setOpen = (next: boolean) => (onToggle ? onToggle(next) : setOwn(next));
  return (
    <section className="panel-section">
      <button className="section-header" aria-expanded={open} onClick={() => setOpen(!open)}>
        {icon}
        <span className="section-name">{title}</span>
        {count && <span className="section-count">{count}</span>}
        {edited && <span className="edited-dot" title="Edited" />}
        <ChevronIcon />
      </button>
      {open && <div className="section-body">{children}</div>}
    </section>
  );
}
