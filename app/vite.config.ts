import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port and must see Rust errors, so don't clear the screen.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  // pdf.js alone is ~1 MB; a desktop app loads it from disk, so the web-oriented warning is noise.
  build: {
    target: "es2022",
    chunkSizeWarningLimit: 2000,
    // Two pages: the main window, and the small floating drop bubble.
    rollupOptions: { input: { main: "index.html", bubble: "bubble.html" } },
  },
});
