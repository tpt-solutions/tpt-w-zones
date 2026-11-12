import { defineConfig } from "vite";

// Tauri expects a fixed dev port and a non-clearing terminal.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: false,
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2021",
    outDir: "../dist",
    emptyOutDir: true,
    sourcemap: false,
  },
});
