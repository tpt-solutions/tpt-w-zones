// Interactive zone canvas: drag on empty space to draw, drag a zone to move it, drag its
// corner to resize, Delete to remove, arrow keys to nudge.

import { h } from "./dom";
import { fromCorners, MIN_SIZE, moved, normalise, pct, resized, SNAP_STEP } from "./geometry";
import type { Layout, NormRect, Zone } from "./types";

export interface CanvasEvents {
  /** The layout's geometry or zone list changed. */
  onChange(): void;
  /** The selection changed. */
  onSelect(zoneId: string | null): void;
}

type Drag =
  | { kind: "create"; ax: number; ay: number; zone: Zone }
  | { kind: "move"; zone: Zone; start: NormRect; px: number; py: number }
  | { kind: "resize"; zone: Zone; start: NormRect; px: number; py: number };

export class LayoutCanvas {
  readonly element: HTMLElement;
  private layout: Layout | null = null;
  private selected: string | null = null;
  private drag: Drag | null = null;
  private counter = 1;

  constructor(private readonly events: CanvasEvents) {
    this.element = h("div", { className: "canvas", tabindex: 0, role: "application" });
    this.element.setAttribute("aria-label", "Zone canvas. Drag to draw, Delete to remove.");
    this.element.addEventListener("pointerdown", (e) => this.onDown(e));
    this.element.addEventListener("pointermove", (e) => this.onMove(e));
    this.element.addEventListener("pointerup", () => this.onUp());
    this.element.addEventListener("pointercancel", () => this.onUp());
    this.element.addEventListener("keydown", (e) => this.onKey(e));
  }

  setLayout(layout: Layout | null): void {
    this.layout = layout;
    this.counter = (layout?.zones.length ?? 0) + 1;
    if (this.selected && !this.zoneById(this.selected)) {
      this.selected = null;
    }
    this.render();
  }

  /** Re-draws after the caller changed a zone's values directly. */
  refresh(): void {
    this.render();
  }

  selectedZone(): Zone | null {
    return this.selected ? this.zoneById(this.selected) : null;
  }

  select(zoneId: string | null): void {
    this.selected = zoneId;
    this.render();
    this.events.onSelect(zoneId);
  }

  /** Adds a default zone in an empty spot and selects it. */
  addZone(): void {
    if (!this.layout) {
      return;
    }
    const zone = this.newZone({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 });
    this.layout.zones.push(zone);
    this.events.onChange();
    this.select(zone.id);
  }

  deleteSelected(): boolean {
    if (!this.layout || !this.selected) {
      return false;
    }
    const id = this.selected;
    this.layout.zones = this.layout.zones.filter((z) => z.id !== id);
    this.selected = null;
    this.events.onChange();
    this.render();
    this.events.onSelect(null);
    return true;
  }

  /** Nudges the selected zone by whole grid steps. */
  nudge(dx: number, dy: number): void {
    const zone = this.selectedZone();
    if (!zone) {
      return;
    }
    zone.rect = moved(zone.rect, dx * SNAP_STEP, dy * SNAP_STEP);
    this.events.onChange();
    this.render();
  }

  private zoneById(id: string): Zone | null {
    return this.layout?.zones.find((z) => z.id === id) ?? null;
  }

  private newZone(rect: NormRect): Zone {
    let id = `zone-${this.counter}`;
    while (this.zoneById(id)) {
      this.counter += 1;
      id = `zone-${this.counter}`;
    }
    this.counter += 1;
    return { id, name: `Zone ${this.counter - 1}`, rect: normalise(rect) };
  }

  private fraction(e: PointerEvent): { x: number; y: number } {
    const box = this.element.getBoundingClientRect();
    return {
      x: Math.min(1, Math.max(0, (e.clientX - box.left) / box.width)),
      y: Math.min(1, Math.max(0, (e.clientY - box.top) / box.height)),
    };
  }

  private onDown(e: PointerEvent): void {
    if (!this.layout || e.button !== 0) {
      return;
    }
    const target = e.target instanceof HTMLElement ? e.target : null;
    const zoneEl = target?.closest<HTMLElement>("[data-zone]") ?? null;
    const zoneId = zoneEl?.dataset["zone"] ?? null;
    const zone = zoneId ? this.zoneById(zoneId) : null;
    const p = this.fraction(e);

    if (zone) {
      this.selected = zone.id;
      const isHandle = target?.dataset["handle"] !== undefined;
      this.drag = isHandle
        ? { kind: "resize", zone, start: { ...zone.rect }, px: p.x, py: p.y }
        : { kind: "move", zone, start: { ...zone.rect }, px: p.x, py: p.y };
      this.events.onSelect(zone.id);
    } else {
      const created = this.newZone({ x: p.x, y: p.y, w: MIN_SIZE, h: MIN_SIZE });
      this.layout.zones.push(created);
      this.selected = created.id;
      this.drag = { kind: "create", ax: p.x, ay: p.y, zone: created };
    }
    this.element.setPointerCapture(e.pointerId);
    this.render();
  }

  private onMove(e: PointerEvent): void {
    if (!this.drag) {
      return;
    }
    const p = this.fraction(e);
    const d = this.drag;
    if (d.kind === "create") {
      d.zone.rect = fromCorners(d.ax, d.ay, p.x, p.y);
    } else if (d.kind === "move") {
      d.zone.rect = moved(d.start, p.x - d.px, p.y - d.py);
    } else {
      d.zone.rect = resized(d.start, p.x - d.px, p.y - d.py);
    }
    this.render();
  }

  private onUp(): void {
    const d = this.drag;
    this.drag = null;
    if (!d || !this.layout) {
      return;
    }
    if (d.kind === "create") {
      // A click without a drag should not leave a stray minimum-size zone behind.
      const tiny = d.zone.rect.w <= MIN_SIZE && d.zone.rect.h <= MIN_SIZE;
      if (tiny) {
        this.layout.zones = this.layout.zones.filter((z) => z !== d.zone);
        this.selected = null;
        this.render();
        this.events.onSelect(null);
        return;
      }
    }
    this.events.onChange();
    this.render();
    this.events.onSelect(this.selected);
  }

  private onKey(e: KeyboardEvent): void {
    if (e.key === "Delete" || e.key === "Backspace") {
      if (this.deleteSelected()) {
        e.preventDefault();
      }
      return;
    }
    const step = e.shiftKey ? 10 : 1;
    const moves: Record<string, [number, number]> = {
      ArrowLeft: [-step, 0],
      ArrowRight: [step, 0],
      ArrowUp: [0, -step],
      ArrowDown: [0, step],
    };
    const move = moves[e.key] as [number, number] | undefined;
    if (move) {
      e.preventDefault();
      this.nudge(move[0], move[1]);
    }
  }

  private render(): void {
    if (!this.layout) {
      this.element.replaceChildren(
        h("p", { className: "muted canvas-empty", text: "Select or create a layout to edit it." }),
      );
      return;
    }
    const zones = this.layout.zones.map((zone) => {
      const el = h(
        "div",
        {
          className: zone.id === this.selected ? "zone selected" : "zone",
          "data-zone": zone.id,
          title: zone.name,
        },
        h("span", { className: "zone-label", text: zone.name }),
        h("span", { className: "zone-handle", "data-handle": "1" }),
      );
      el.style.left = pct(zone.rect.x);
      el.style.top = pct(zone.rect.y);
      el.style.width = pct(zone.rect.w);
      el.style.height = pct(zone.rect.h);
      return el;
    });
    this.element.replaceChildren(...zones);
  }
}
