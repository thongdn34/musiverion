import type { HTMLAttributes } from "react";
import "./GlassPanel.css";

export type GlassPanelProps = HTMLAttributes<HTMLDivElement>;

/** Blue Glass panel surface: glass fill, 1px border, top highlight, backdrop blur. */
export function GlassPanel({ className, ...rest }: GlassPanelProps) {
  const classes = className ? `mv-glass-panel ${className}` : "mv-glass-panel";
  return <div className={classes} {...rest} />;
}
