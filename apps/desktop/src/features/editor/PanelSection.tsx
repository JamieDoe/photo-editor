import { useState, type ReactNode } from "react";
import { ChevronIcon } from "../../components/icons";

interface Props {
  title: string;
  icon: ReactNode;
  /** Shows the accent dot: something in this section differs from its default. */
  edited?: boolean;
  defaultOpen?: boolean;
  children: ReactNode;
}

/** A collapsible section of the right-hand panel. */
export function PanelSection({ title, icon, edited = false, defaultOpen = true, children }: Props) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <section className="panel-section">
      <button className="section-header" aria-expanded={open} onClick={() => setOpen(!open)}>
        {icon}
        <span className="section-name">{title}</span>
        {edited && <span className="edited-dot" title="Edited" />}
        <ChevronIcon />
      </button>
      {open && <div className="section-body">{children}</div>}
    </section>
  );
}
