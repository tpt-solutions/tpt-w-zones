// Shapes mirror the JSON produced by zones-core's serde derives.

export interface NormRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Zone {
  id: string;
  name: string;
  rect: NormRect;
}

export interface Layout {
  id: string;
  name: string;
  zones: Zone[];
}

export interface Profile {
  name: string;
  monitors: Record<string, string>;
}

export interface Rule {
  name?: string;
  process?: string;
  class?: string;
  title_contains?: string;
  profile?: string;
  monitor?: string;
  layout: string;
  zone: string;
}

export interface General {
  autostart: boolean;
  log_level: string;
  crash_reports: boolean;
}

export interface Config {
  schema_version: number;
  active_profile?: string;
  general: General;
  hotkeys: Record<string, string>;
  layouts: Layout[];
  profiles: Profile[];
  rules: Rule[];
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Monitor {
  id: number;
  name: string;
  bounds: Rect;
  work_area: Rect;
  scale_factor: number;
  primary: boolean;
}

export interface AppInfo {
  version: string;
  platform: string;
  config_schema: number;
  protocol: number;
  config_path: string | null;
}
