import { invoke } from "@tauri-apps/api/core";

interface AppInfo {
  version: string;
  platform: string;
  config_schema: number;
  protocol: number;
}

function setText(id: string, value: string): void {
  const el = document.getElementById(id);
  if (el) {
    el.textContent = value;
  }
}

async function loadAppInfo(): Promise<void> {
  try {
    const info = await invoke<AppInfo>("app_info");
    setText("info-version", info.version);
    setText("info-platform", info.platform);
    setText("info-schema", String(info.config_schema));
    setText("info-protocol", String(info.protocol));
  } catch (err) {
    // Running in a plain browser (vite dev without Tauri) has no IPC bridge.
    setText("info-version", `unavailable (${String(err)})`);
  }
}

void loadAppInfo();
