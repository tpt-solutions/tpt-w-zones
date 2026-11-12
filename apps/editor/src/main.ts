import { invoke } from "@tauri-apps/api/core";
import { byButton, byId, h } from "./dom";
import { LayoutCanvas } from "./layout-canvas";
import { pct } from "./geometry";
import { renderPreview } from "./preview";
import type { AppInfo, Config, Layout, Monitor, Profile } from "./types";

type Tab = "layouts" | "profiles" | "settings";

interface State {
  config: Config;
  /** JSON of the last saved (or loaded) config; the working copy is dirty when it differs. */
  saved: string;
  tab: Tab;
  layoutId: string | null;
  profileName: string | null;
  monitors: Monitor[] | null;
  monitorError: string | null;
  validation: string | null;
  status: string;
  configPath: string | null;
}

const LOG_LEVELS = ["error", "warn", "info", "debug", "trace"];
const EMPTY_CONFIG: Config = {
  schema_version: 1,
  general: { autostart: false, log_level: "info" },
  hotkeys: {},
  layouts: [],
  profiles: [],
  rules: [],
};

const state: State = {
  config: structuredClone(EMPTY_CONFIG),
  saved: JSON.stringify(EMPTY_CONFIG),
  tab: "layouts",
  layoutId: null,
  profileName: null,
  monitors: null,
  monitorError: null,
  validation: null,
  status: "Loading…",
  configPath: null,
};

const canvas = new LayoutCanvas({
  onChange: () => {
    changed();
  },
  onSelect: () => {
    renderZoneForm();
  },
});

let validateTimer: number | undefined;

/// The hotkey action waiting for a key combination, if any.
let recording: string | null = null;

/// Maps KeyboardEvent.code values to the key names zones-core accepts.
const CODE_NAMES: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  Tab: "Tab",
  Escape: "Escape",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  ArrowUp: "Up",
  ArrowDown: "Down",
};

/** Key name for a physical key code, or null for keys we cannot bind. */
function keyName(code: string): string | null {
  const named = CODE_NAMES[code] as string | undefined;
  if (named !== undefined) {
    return named;
  }
  if (/^Key[A-Z]$/.test(code)) {
    return code.slice(3);
  }
  if (/^Digit[0-9]$/.test(code)) {
    return code.slice(5);
  }
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) {
    return code;
  }
  return null;
}

/** Builds a hotkey string such as "Ctrl+Alt+Left", or null for modifier-only presses. */
function comboFrom(e: KeyboardEvent): string | null {
  const key = keyName(e.code);
  if (key === null) {
    return null;
  }
  const parts: string[] = [];
  if (e.metaKey) {
    parts.push("Win");
  }
  if (e.ctrlKey) {
    parts.push("Ctrl");
  }
  if (e.altKey) {
    parts.push("Alt");
  }
  if (e.shiftKey) {
    parts.push("Shift");
  }
  return [...parts, key].join("+");
}

// ---------- commands ----------

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

async function loadFromDisk(): Promise<void> {
  try {
    const config = await call<Config>("load_config");
    setConfig(config, true);
    state.status = "Loaded configuration.";
  } catch (err) {
    state.status = `Could not load: ${errorText(err)}`;
  }
  render();
}

async function save(): Promise<void> {
  try {
    await call<null>("validate_config", { config: state.config });
  } catch (err) {
    state.validation = errorText(err);
    state.status = "Not saved: fix the error first.";
    render();
    return;
  }
  try {
    await call<null>("save_config", { config: state.config });
    state.saved = JSON.stringify(state.config);
    state.validation = null;
    state.status = `Saved to ${state.configPath ?? "the config file"}.`;
  } catch (err) {
    state.status = `Save failed: ${errorText(err)}`;
  }
  render();
}

async function refreshMonitors(): Promise<void> {
  try {
    state.monitors = await call<Monitor[]>("list_monitors");
    state.monitorError = null;
  } catch (err) {
    state.monitors = null;
    state.monitorError = errorText(err);
  }
  renderProfiles();
}

function scheduleValidation(): void {
  window.clearTimeout(validateTimer);
  validateTimer = window.setTimeout(() => {
    void call<null>("validate_config", { config: state.config })
      .then(() => {
        state.validation = null;
      })
      .catch((err: unknown) => {
        state.validation = errorText(err);
      })
      .finally(() => {
        renderStatus();
      });
  }, 250);
}

/** Called after any edit to the working copy. */
function changed(): void {
  renderStatus();
  scheduleValidation();
}

function setConfig(config: Config, markSaved: boolean): void {
  state.config = structuredClone(config);
  if (markSaved) {
    state.saved = JSON.stringify(config);
  }
  if (!state.config.layouts.some((l) => l.id === state.layoutId)) {
    state.layoutId = state.config.layouts[0]?.id ?? null;
  }
  if (!state.config.profiles.some((p) => p.name === state.profileName)) {
    state.profileName = state.config.profiles[0]?.name ?? null;
  }
}

function isDirty(): boolean {
  return JSON.stringify(state.config) !== state.saved;
}

function currentLayout(): Layout | null {
  return state.config.layouts.find((l) => l.id === state.layoutId) ?? null;
}

function currentProfile(): Profile | null {
  return state.config.profiles.find((p) => p.name === state.profileName) ?? null;
}

function uniqueName(base: string, taken: string[]): string {
  let candidate = base;
  let n = 2;
  while (taken.includes(candidate)) {
    candidate = `${base} ${n}`;
    n += 1;
  }
  return candidate;
}

// ---------- layouts tab ----------

function layoutsPane(): HTMLElement {
  return byId("pane-layouts");
}

function renderLayouts(): void {
  const pane = layoutsPane();
  const layout = currentLayout();

  const list = h(
    "ul",
    { className: "item-list" },
    ...state.config.layouts.map((l) =>
      h(
        "li",
        {},
        h("button", {
          className: l.id === state.layoutId ? "item active" : "item",
          text: `${l.name} (${l.zones.length})`,
          onclick: () => {
            state.layoutId = l.id;
            canvas.select(null);
            renderLayouts();
          },
        }),
      ),
    ),
  );

  const sidebar = h(
    "aside",
    { className: "sidebar" },
    h("h2", { text: "Layouts" }),
    list,
    h(
      "div",
      { className: "button-row" },
      h("button", { text: "New", onclick: newLayout }),
      h("button", { text: "Duplicate", disabled: !layout, onclick: duplicateLayout }),
      h("button", {
        className: "danger",
        text: "Delete",
        disabled: !layout,
        onclick: deleteLayout,
      }),
    ),
  );

  const main = layout
    ? h(
        "section",
        { className: "editor-main" },
        h(
          "label",
          { className: "field" },
          "Name",
          h("input", {
            type: "text",
            value: layout.name,
            oninput: (e: Event) => {
              layout.name = (e.target as HTMLInputElement).value;
              changed();
            },
          }),
        ),
        h(
          "p",
          { className: "muted" },
          "Drag on empty space to draw a zone. Drag a zone to move it, or its corner to resize. ",
          "Delete removes the selected zone; arrow keys nudge it (Shift for larger steps).",
        ),
        canvas.element,
        h("div", { id: "zone-form", className: "zone-form" }),
      )
    : h(
        "section",
        { className: "editor-main" },
        h("p", { className: "muted", text: "No layout selected. Create one with New." }),
      );

  pane.replaceChildren(h("div", { className: "split" }, sidebar, main));
  canvas.setLayout(layout);
  renderZoneForm();
}

function renderZoneForm(): void {
  const host = document.getElementById("zone-form");
  if (!host) {
    return;
  }
  const layout = currentLayout();
  const zone = canvas.selectedZone();
  if (!layout) {
    host.replaceChildren();
    return;
  }
  // A keyboard and screen-reader alternative to clicking zones on the canvas.
  const zoneList = h(
    "ul",
    { className: "zone-list", "aria-label": "Zones in this layout" },
    ...layout.zones.map((z) =>
      h(
        "li",
        {},
        h("button", {
          className: z.id === zone?.id ? "item active" : "item",
          "aria-pressed": String(z.id === zone?.id),
          text: z.name,
          onclick: () => {
            canvas.select(z.id);
          },
        }),
      ),
    ),
  );
  if (!zone) {
    host.replaceChildren(
      h("p", {
        className: "muted",
        text: `${layout.zones.length} zone(s). Select one to edit its numbers.`,
      }),
      h("button", {
        text: "Add zone",
        onclick: () => {
          canvas.addZone();
        },
      }),
    );
    return;
  }
  const numberField = (label: string, value: number, apply: (v: number) => void): HTMLElement =>
    h(
      "label",
      { className: "field small" },
      label,
      h("input", {
        type: "number",
        min: 0,
        max: 100,
        step: 0.5,
        value: pct(value).replace("%", ""),
        onchange: (e: Event) => {
          const v = Number((e.target as HTMLInputElement).value);
          if (Number.isFinite(v)) {
            apply(v / 100);
            canvas.refresh();
            changed();
          }
        },
      }),
    );

  host.replaceChildren(
    zoneList,
    h(
      "div",
      { className: "zone-fields" },
      h(
        "label",
        { className: "field" },
        "Zone name",
        h("input", {
          type: "text",
          value: zone.name,
          oninput: (e: Event) => {
            zone.name = (e.target as HTMLInputElement).value;
            canvas.refresh();
            changed();
          },
        }),
      ),
      h(
        "label",
        { className: "field" },
        "Zone id",
        h("input", {
          type: "text",
          value: zone.id,
          onchange: (e: Event) => {
            zone.id = (e.target as HTMLInputElement).value.trim();
            changed();
            renderLayouts();
          },
        }),
      ),
      numberField("X %", zone.rect.x, (v) => {
        zone.rect = { ...zone.rect, x: v };
      }),
      numberField("Y %", zone.rect.y, (v) => {
        zone.rect = { ...zone.rect, y: v };
      }),
      numberField("Width %", zone.rect.w, (v) => {
        zone.rect = { ...zone.rect, w: v };
      }),
      numberField("Height %", zone.rect.h, (v) => {
        zone.rect = { ...zone.rect, h: v };
      }),
      h("button", {
        className: "danger",
        text: "Delete zone",
        onclick: () => {
          canvas.deleteSelected();
        },
      }),
    ),
  );
}

function newLayout(): void {
  const name = uniqueName(
    "New layout",
    state.config.layouts.map((l) => l.name),
  );
  const layout: Layout = {
    id: slug(
      name,
      state.config.layouts.map((l) => l.id),
    ),
    name,
    zones: [{ id: "main", name: "Main", rect: { x: 0, y: 0, w: 1, h: 1 } }],
  };
  state.config.layouts.push(layout);
  state.layoutId = layout.id;
  changed();
  renderLayouts();
}

function duplicateLayout(): void {
  const source = currentLayout();
  if (!source) {
    return;
  }
  const ids = state.config.layouts.map((l) => l.id);
  const copy: Layout = structuredClone(source);
  copy.name = uniqueName(
    `${source.name} copy`,
    state.config.layouts.map((l) => l.name),
  );
  copy.id = slug(copy.name, ids);
  state.config.layouts.push(copy);
  state.layoutId = copy.id;
  changed();
  renderLayouts();
}

function deleteLayout(): void {
  const layout = currentLayout();
  if (!layout) {
    return;
  }
  if (
    !window.confirm(
      `Delete layout "${layout.name}"? Profiles and rules that use it will fail validation until fixed.`,
    )
  ) {
    return;
  }
  state.config.layouts = state.config.layouts.filter((l) => l.id !== layout.id);
  state.layoutId = state.config.layouts[0]?.id ?? null;
  changed();
  renderLayouts();
}

/** Turns a display name into a unique, file-friendly id. */
function slug(name: string, taken: string[]): string {
  const base =
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "") || "layout";
  let candidate = base;
  let n = 2;
  while (taken.includes(candidate)) {
    candidate = `${base}-${n}`;
    n += 1;
  }
  return candidate;
}

// ---------- profiles tab ----------

function renderProfiles(): void {
  const pane = byId("pane-profiles");
  const profile = currentProfile();
  const layouts = state.config.layouts;
  const monitors = state.monitors ?? [];

  const activeSelect = h(
    "select",
    {
      onchange: (e: Event) => {
        const value = (e.target as HTMLSelectElement).value;
        state.config.active_profile = value === "" ? undefined : value;
        changed();
        renderProfiles();
      },
    },
    h("option", { value: "", text: "(none)", selected: !state.config.active_profile }),
    ...state.config.profiles.map((p) =>
      h("option", {
        value: p.name,
        text: p.name,
        selected: p.name === state.config.active_profile,
      }),
    ),
  );

  const profileList = h(
    "ul",
    { className: "item-list" },
    ...state.config.profiles.map((p) =>
      h(
        "li",
        {},
        h("button", {
          className: p.name === state.profileName ? "item active" : "item",
          text: p.name,
          onclick: () => {
            state.profileName = p.name;
            renderProfiles();
          },
        }),
      ),
    ),
  );

  const sidebar = h(
    "aside",
    { className: "sidebar" },
    h("h2", { text: "Profiles" }),
    profileList,
    h(
      "div",
      { className: "button-row" },
      h("button", { text: "New", onclick: newProfile }),
      h("button", {
        className: "danger",
        text: "Delete",
        disabled: !profile,
        onclick: deleteProfile,
      }),
    ),
  );

  const mappingRows = profile
    ? Object.entries(profile.monitors).map(([monitorName, layoutId]) =>
        h(
          "tr",
          {},
          h(
            "td",
            {},
            h("input", {
              type: "text",
              value: monitorName,
              list: "monitor-names",
              onchange: (e: Event) => {
                const next = (e.target as HTMLInputElement).value.trim();
                renameMonitorKey(profile, monitorName, next);
                changed();
                renderProfiles();
              },
            }),
          ),
          h(
            "td",
            {},
            h(
              "select",
              {
                onchange: (e: Event) => {
                  profile.monitors[monitorName] = (e.target as HTMLSelectElement).value;
                  changed();
                  renderPreviewOnly();
                },
              },
              ...layouts.map((l) =>
                h("option", { value: l.id, text: l.name, selected: l.id === layoutId }),
              ),
              ...(layouts.some((l) => l.id === layoutId)
                ? []
                : [h("option", { value: layoutId, text: `missing: ${layoutId}`, selected: true })]),
            ),
          ),
          h(
            "td",
            {},
            h("button", {
              className: "danger",
              text: "Remove",
              onclick: () => {
                changed();
                renderProfiles();
              },
            }),
          ),
        ),
      )
    : [];

  const datalist = h(
    "datalist",
    { id: "monitor-names" },
    ...monitors.map((m) => h("option", { value: m.name })),
  );

  const mapping = profile
    ? h(
        "section",
        { className: "profile-main" },
        h(
          "label",
          { className: "field" },
          "Profile name",
          h("input", {
            type: "text",
            value: profile.name,
            onchange: (e: Event) => {
              renameProfile(profile, (e.target as HTMLInputElement).value.trim());
              renderProfiles();
            },
          }),
        ),
        h("h3", { text: "Monitor → layout" }),
        h(
          "p",
          { className: "muted" },
          "Monitors are matched by their stable name (for example DISPLAY1). Pick a detected monitor or type a name.",
        ),
        h(
          "table",
          { className: "mapping" },
          h(
            "thead",
            {},
            h("tr", {}, h("th", { text: "Monitor" }), h("th", { text: "Layout" }), h("th", {})),
          ),
          h("tbody", {}, ...mappingRows),
        ),
        datalist,
        h("button", {
          text: "Add monitor",
          onclick: () => {
            addMonitorRow(profile);
            changed();
            renderProfiles();
          },
        }),
        h("h3", { text: "Preview" }),
        h("p", {
          className: "muted",
          text: "Show this profile's zones on the real monitors for a few seconds. Unsaved edits are included.",
        }),
        h("button", {
          text: "Show on monitors (5 s)",
          onclick: () => {
            showOnMonitors();
          },
        }),
        h("div", { id: "preview", className: "preview" }),
      )
    : h(
        "section",
        { className: "profile-main" },
        h("p", { className: "muted", text: "No profile selected. Create one with New." }),
      );

  pane.replaceChildren(
    h(
      "div",
      { className: "profile-top" },
      h("label", { className: "field" }, "Active profile", activeSelect),
      h("button", {
        text: "Refresh monitors",
        onclick: () => {
          void refreshMonitors();
        },
      }),
      state.monitorError
        ? h("span", { className: "notice warn", text: `Monitors: ${state.monitorError}` })
        : null,
    ),
    h("div", { className: "split" }, sidebar, mapping),
  );
  renderPreviewOnly();
}

function showOnMonitors(): void {
  call<null>("show_preview", {
    config: state.config,
    profile: state.profileName,
    seconds: 5,
  }).catch((err: unknown) => {
    state.status = `Preview failed: ${errorText(err)}`;
    renderStatus();
  });
}

function renderPreviewOnly(): void {
  const host = document.getElementById("preview");
  if (host) {
    renderPreview(host, state.config, currentProfile(), state.monitors, state.monitorError);
  }
}

function newProfile(): void {
  const name = uniqueName(
    "New profile",
    state.config.profiles.map((p) => p.name),
  );
  state.config.profiles.push({ name, monitors: {} });
  state.profileName = name;
  changed();
  renderProfiles();
}

function deleteProfile(): void {
  const profile = currentProfile();
  if (!profile) {
    return;
  }
  if (!window.confirm(`Delete profile "${profile.name}"?`)) {
    return;
  }
  state.config.profiles = state.config.profiles.filter((p) => p !== profile);
  if (state.config.active_profile === profile.name) {
    state.config.active_profile = undefined;
  }
  state.profileName = state.config.profiles[0]?.name ?? null;
  changed();
  renderProfiles();
}

function renameProfile(profile: Profile, name: string): void {
  if (!name || state.config.profiles.some((p) => p !== profile && p.name === name)) {
    return;
  }
  if (state.config.active_profile === profile.name) {
    state.config.active_profile = name;
  }
  profile.name = name;
  state.profileName = name;
  changed();
}

function renameMonitorKey(profile: Profile, oldName: string, newName: string): void {
  if (!newName || newName === oldName) {
    return;
  }
  const entries: [string, string][] = Object.entries(profile.monitors).map(([k, v]) =>
    k === oldName ? [newName, v] : [k, v],
  );
  profile.monitors = Object.fromEntries(entries);
}

function addMonitorRow(profile: Profile): void {
  const used = Object.keys(profile.monitors);
  const detected = (state.monitors ?? []).map((m) => m.name).find((n) => !used.includes(n));
  const name = detected ?? uniqueName("MONITOR", used);
  const layoutId = state.config.layouts[0]?.id ?? "";
  profile.monitors[name] = layoutId;
}

// ---------- settings tab ----------

function renderSettings(): void {
  const pane = byId("pane-settings");
  const hotkeyRows = Object.entries(state.config.hotkeys).map(([action, hotkey]) =>
    h(
      "tr",
      {},
      h(
        "td",
        {},
        h("input", {
          type: "text",
          value: action,
          onchange: (e: Event) => {
            const next = (e.target as HTMLInputElement).value.trim();
            if (next && next !== action && !(next in state.config.hotkeys)) {
              state.config.hotkeys = renameKey(state.config.hotkeys, action, next);
            }
            changed();
            renderSettings();
          },
        }),
      ),
      h(
        "td",
        {},
        h("input", {
          type: "text",
          value: hotkey,
          placeholder: "Ctrl+Alt+Left",
          "aria-label": `Hotkey for `,
          oninput: (e: Event) => {
            state.config.hotkeys[action] = (e.target as HTMLInputElement).value.trim();
            changed();
          },
        }),
        h("button", {
          text: recording === action ? "Press keys… (Esc cancels)" : "Record",
          "aria-label": `Record hotkey for `,
          "aria-pressed": String(recording === action),
          onclick: () => {
            recording = recording === action ? null : action;
            renderSettings();
          },
        }),
      ),
      h(
        "td",
        {},
        h("button", {
          className: "danger",
          text: "Remove",
          onclick: () => {
            state.config.hotkeys = withoutKey(state.config.hotkeys, action);
            changed();
            renderSettings();
          },
        }),
      ),
    ),
  );

  pane.replaceChildren(
    h(
      "section",
      { className: "settings" },
      h("h2", { text: "General" }),
      h(
        "label",
        { className: "field inline" },
        h("input", {
          type: "checkbox",
          checked: state.config.general.autostart,
          onchange: (e: Event) => {
            state.config.general.autostart = (e.target as HTMLInputElement).checked;
            changed();
          },
        }),
        "Start tpt-zones when I sign in",
      ),
      h(
        "label",
        { className: "field" },
        "Log level",
        h(
          "select",
          {
            onchange: (e: Event) => {
              state.config.general.log_level = (e.target as HTMLSelectElement).value;
              changed();
            },
          },
          ...LOG_LEVELS.map((level) =>
            h("option", {
              value: level,
              text: level,
              selected: level === state.config.general.log_level,
            }),
          ),
        ),
      ),
      h("h2", { text: "Hotkeys" }),
      h(
        "p",
        { className: "muted" },
        "Action names: snap:LAYOUT:ZONE (for example snap:halves:left), apply_rules, reload. ",
        "Keys use the form Ctrl+Alt+Left, Win+Shift+1, F13.",
      ),
      h(
        "table",
        { className: "mapping" },
        h(
          "thead",
          {},
          h("tr", {}, h("th", { text: "Action" }), h("th", { text: "Hotkey" }), h("th", {})),
        ),
        h("tbody", {}, ...hotkeyRows),
      ),
      h("button", {
        text: "Add hotkey",
        onclick: () => {
          const action = uniqueName("snap:LAYOUT:ZONE", Object.keys(state.config.hotkeys));
          state.config.hotkeys[action] = "";
          changed();
          renderSettings();
        },
      }),
      h("h2", { text: "Rules" }),
      h(
        "p",
        { className: "muted" },
        `${state.config.rules.length} placement rule(s). Rules are kept as written; edit them in config.toml (see docs/scripting.md).`,
      ),
    ),
  );
}

function withoutKey<V>(record: Record<string, V>, key: string): Record<string, V> {
  return Object.fromEntries(Object.entries(record).filter(([k]) => k !== key));
}

function renameKey(
  record: Record<string, string>,
  from: string,
  to: string,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(record).map(([k, v]): [string, string] => (k === from ? [to, v] : [k, v])),
  );
}

// ---------- chrome ----------

function renderStatus(): void {
  const dirty = isDirty();
  byId("dirty").textContent = dirty ? "Unsaved changes" : "All changes saved";
  byId("dirty").className = dirty ? "dirty" : "clean";
  byId("validation").textContent = state.validation ?? "Configuration is valid.";
  byId("validation").className = state.validation ? "notice warn" : "notice ok";
  byId("status").textContent = state.status;
  byButton("save").disabled = !dirty;
  byButton("revert").disabled = !dirty;
}

function render(): void {
  for (const tab of ["layouts", "profiles", "settings"] as const) {
    byId(`pane-${tab}`).hidden = state.tab !== tab;
    byButton(`tab-${tab}`).setAttribute("aria-selected", String(state.tab === tab));
  }
  renderLayouts();
  renderProfiles();
  renderSettings();
  renderStatus();
}

function selectTab(tab: Tab): void {
  state.tab = tab;
  render();
  if (tab === "profiles" && state.monitors === null && state.monitorError === null) {
    void refreshMonitors();
  }
}

async function init(): Promise<void> {
  for (const tab of ["layouts", "profiles", "settings"] as const) {
    byButton(`tab-${tab}`).addEventListener("click", () => {
      selectTab(tab);
    });
  }
  byButton("save").addEventListener("click", () => {
    void save();
  });
  byButton("revert").addEventListener("click", () => {
    setConfig(JSON.parse(state.saved) as Config, false);
    state.validation = null;
    state.status = "Reverted to the saved configuration.";
    render();
  });
  byButton("reload").addEventListener("click", () => {
    if (isDirty() && !window.confirm("Discard unsaved changes and reload from disk?")) {
      return;
    }
    void loadFromDisk();
  });
  document.addEventListener("keydown", (e) => {
    if (recording === null) {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape") {
      recording = null;
      renderSettings();
      return;
    }
    const combo = comboFrom(e);
    if (combo === null) {
      return; // a modifier on its own: keep waiting for the key
    }
    const action = recording;
    recording = null;
    state.config.hotkeys[action] = combo;
    changed();
    renderSettings();
  });

  document.addEventListener("keydown", (e) => {
    if (recording !== null) {
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save();
    }
  });

  try {
    const info = await call<AppInfo>("app_info");
    state.configPath = info.config_path;
    byId("config-path").textContent = info.config_path ?? "";
  } catch {
    state.configPath = null;
  }
  await loadFromDisk();
  render();
}

void init();
