// Live preview: detected monitors, each showing the zones of the layout the selected
// profile assigns to it. Read-only; editing happens on the canvas.

import { h } from "./dom";
import { pct } from "./geometry";
import type { Config, Monitor, Profile } from "./types";

export function renderPreview(
  host: HTMLElement,
  config: Config,
  profile: Profile | null,
  monitors: Monitor[] | null,
  error: string | null,
): void {
  if (error) {
    host.replaceChildren(
      h("p", { className: "notice warn", text: `Monitors unavailable: ${error}` }),
    );
    return;
  }
  if (!monitors) {
    host.replaceChildren(h("p", { className: "muted", text: "Detecting monitors…" }));
    return;
  }
  if (monitors.length === 0) {
    host.replaceChildren(h("p", { className: "muted", text: "No monitors detected." }));
    return;
  }

  const cards = monitors.map((monitor) => {
    const layoutId = profile?.monitors[monitor.name];
    const layout = layoutId ? config.layouts.find((l) => l.id === layoutId) : undefined;
    const surface = h("div", { className: "monitor-surface" });
    surface.style.aspectRatio = `${monitor.work_area.width} / ${monitor.work_area.height}`;
    if (layout) {
      for (const zone of layout.zones) {
        const zoneEl = h("div", { className: "preview-zone", title: zone.name }, zone.name);
        zoneEl.style.left = pct(zone.rect.x);
        zoneEl.style.top = pct(zone.rect.y);
        zoneEl.style.width = pct(zone.rect.w);
        zoneEl.style.height = pct(zone.rect.h);
        surface.append(zoneEl);
      }
    }
    const caption = `${monitor.name}${monitor.primary ? " (primary)" : ""} · ${monitor.work_area.width}×${monitor.work_area.height} · ${Math.round(monitor.scale_factor * 100)}%`;
    const note = layoutId
      ? layout
        ? `layout: ${layout.name}`
        : `profile points at missing layout "${layoutId}"`
      : "no layout assigned in this profile";
    return h(
      "figure",
      { className: "monitor-card" },
      surface,
      h(
        "figcaption",
        {},
        h("strong", { text: caption }),
        h("span", { className: "muted", text: note }),
      ),
    );
  });
  host.replaceChildren(...cards);
}
