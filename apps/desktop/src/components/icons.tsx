/**
 * Line icons from the app design: 16-unit grid, 1.5 stroke, round caps. Decorative
 * by default (aria-hidden); the surrounding control carries the accessible name.
 */
import type { ReactNode } from "react";

interface IconProps {
  size?: number;
  strokeWidth?: number;
}

function Icon({ size = 16, strokeWidth, children }: IconProps & { children: ReactNode }) {
  return (
    <svg className="icon" width={size} height={size} viewBox="0 0 16 16" aria-hidden="true" style={strokeWidth ? { strokeWidth } : undefined}>
      {children}
    </svg>
  );
}

export const ExportIcon = (p: IconProps) => (
  <Icon size={15} strokeWidth={1.7} {...p}>
    <path d="M8 10V2M5 4.5 8 1.5l3 3" />
    <path d="M3 8.5v4A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5v-4" />
  </Icon>
);

export const ImportIcon = (p: IconProps) => (
  <Icon size={15} {...p}>
    <path d="M8 2v8M5 7l3 3 3-3" />
    <path d="M3 12.5h10" />
  </Icon>
);

export const FolderIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3L8 4.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
  </Icon>
);

export const PhotosIcon = (p: IconProps) => (
  <Icon {...p}>
    <rect x="2" y="3" width="12" height="10" rx="2" />
    <path d="M2.5 11 6 7.5l3 3 2-2 2.5 2.5" />
  </Icon>
);

export const ChevronIcon = ({ size = 14 }: IconProps) => (
  <svg className="icon chevron" width={size} height={size} viewBox="0 0 16 16" aria-hidden="true">
    <path d="M4 6l4 4 4-4" />
  </svg>
);

export const LightIcon = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="8" cy="8" r="2.8" />
    <path d="M8 1.5V3M8 13v1.5M1.5 8H3M13 8h1.5M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1" />
  </Icon>
);

export const ColourIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M8 2s4.5 4.6 4.5 7.7a4.5 4.5 0 0 1-9 0C3.5 6.6 8 2 8 2z" />
  </Icon>
);

export const DetailIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M2 5V3a1 1 0 0 1 1-1h2M11 2h2a1 1 0 0 1 1 1v2M14 11v2a1 1 0 0 1-1 1h-2M5 14H3a1 1 0 0 1-1-1v-2" />
    <circle cx="8" cy="8" r="2.2" />
  </Icon>
);

export const SettingsIcon = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="8" cy="8" r="2" />
    <path d="M8 1.8v1.6M8 12.6v1.6M1.8 8h1.6M12.6 8h1.6M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M3.6 12.4l1.1-1.1M11.3 4.7l1.1-1.1" />
  </Icon>
);

export const RefreshIcon = (p: IconProps) => (
  <Icon size={14} {...p}>
    <path d="M13 8a5 5 0 1 1-1.5-3.6" />
    <path d="M13 2.5v3h-3" />
  </Icon>
);

export const OpenIcon = (p: IconProps) => (
  <Icon size={15} {...p}>
    <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3L8 4.5h4.5A1.5 1.5 0 0 1 14 6v1" />
    <path d="M2 4.5v7A1.5 1.5 0 0 0 3.5 13h8.2a1.5 1.5 0 0 0 1.4-1l1.3-3.5a.8.8 0 0 0-.7-1.1H5.2a1.5 1.5 0 0 0-1.4 1L2 13" />
  </Icon>
);

export const StarIcon = (p: IconProps) => (
  <Icon size={12} {...p}>
    <path d="M8 2l1.8 3.7 4 .6-2.9 2.8.7 4L8 11.2 4.4 13.1l.7-4-2.9-2.8 4-.6z" />
  </Icon>
);

/** Rating star; filled stars use the accent. */
export const RatingStar = ({ filled }: { filled: boolean }) => (
  <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" className={filled ? "rating-star filled" : "rating-star"}>
    <path d="M8 2l1.8 3.8 4.2.5-3.1 2.9.8 4.1L8 11.3l-3.7 2 .8-4.1L2 6.3l4.2-.5z" />
  </svg>
);

export const PickIcon = ({ size = 15, filled = false }: IconProps & { filled?: boolean }) => (
  <svg className="icon" width={size} height={size} viewBox="0 0 16 16" aria-hidden="true">
    <path d="M3.5 14V2.5M3.5 3h8l-1.8 3 1.8 3h-8" style={filled ? { fill: "currentColor" } : undefined} />
  </svg>
);

export const RejectIcon = (p: IconProps) => (
  <Icon size={15} {...p}>
    <circle cx="8" cy="8" r="6" />
    <path d="M5.5 5.5l5 5M10.5 5.5l-5 5" />
  </Icon>
);

export const DiagnosticsIcon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M1.5 8h3l1.5-4 3 8 1.5-4h4" />
  </Icon>
);

/** Placeholder brand mark (the product name is not decided yet). */
export const BrandMark = () => (
  <svg width="22" height="22" viewBox="0 0 22 22" aria-hidden="true">
    <circle cx="11" cy="11" r="8" fill="var(--accent)" />
    <circle cx="14.2" cy="7.8" r="5.4" fill="var(--surface)" />
  </svg>
);
