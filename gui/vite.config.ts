import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri expects the dev server on a fixed port.
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
});
