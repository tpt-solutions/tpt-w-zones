// Pure geometry for normalised zone rectangles (fractions of the work area).
// Kept free of DOM access so the drag rules can be read and tested on their own.

import type { NormRect } from "./types";

/** Smallest zone edge the editor will produce, as a fraction of the work area. */
export const MIN_SIZE = 0.02;

/** Pointer positions snap to this step, so zones line up on a readable grid. */
export const SNAP_STEP = 0.005;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function snap(value: number): number {
  return Math.round(value / SNAP_STEP) * SNAP_STEP;
}

/** Rounds away binary floating-point noise so saved files stay tidy. */
export function tidy(value: number): number {
  return Math.round(value * 100000) / 100000;
}

export function normalise(rect: NormRect): NormRect {
  return {
    x: tidy(clamp(rect.x, 0, 1)),
    y: tidy(clamp(rect.y, 0, 1)),
    w: tidy(clamp(rect.w, MIN_SIZE, 1)),
    h: tidy(clamp(rect.h, MIN_SIZE, 1)),
  };
}

/** Keeps a rectangle inside the unit square without changing its size. */
export function keepInside(rect: NormRect): NormRect {
  const x = clamp(rect.x, 0, 1 - rect.w);
  const y = clamp(rect.y, 0, 1 - rect.h);
  return { x: tidy(x), y: tidy(y), w: rect.w, h: rect.h };
}

/** Rectangle spanned by two corner points, snapped and clamped. */
export function fromCorners(ax: number, ay: number, bx: number, by: number): NormRect {
  const x0 = snap(clamp(Math.min(ax, bx), 0, 1));
  const y0 = snap(clamp(Math.min(ay, by), 0, 1));
  const x1 = snap(clamp(Math.max(ax, bx), 0, 1));
  const y1 = snap(clamp(Math.max(ay, by), 0, 1));
  return normalise({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
}

/** Moves a rectangle by a pointer delta, staying inside the work area. */
export function moved(start: NormRect, dx: number, dy: number): NormRect {
  return keepInside({
    x: snap(start.x + dx),
    y: snap(start.y + dy),
    w: start.w,
    h: start.h,
  });
}

/** Resizes from the bottom-right corner, keeping the top-left fixed. */
export function resized(start: NormRect, dx: number, dy: number): NormRect {
  const w = clamp(snap(start.w + dx), MIN_SIZE, 1 - start.x);
  const h = clamp(snap(start.h + dy), MIN_SIZE, 1 - start.y);
  return { x: start.x, y: start.y, w: tidy(w), h: tidy(h) };
}

/** Percentage string for CSS, e.g. 0.25 -> "25%". */
export function pct(fraction: number): string {
  return `${tidy(fraction * 100)}%`;
}
