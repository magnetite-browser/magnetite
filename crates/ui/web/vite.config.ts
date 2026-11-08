import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Built assets are served by the browser at magnetite://ui/, so paths must be relative.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "./",
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false, target: "chrome130" },
  // The licence texts come from the repository root (`Licenses.tsx`).
  server: { port: 5173, strictPort: true, fs: { allow: [fileURLToPath(new URL("../../..", import.meta.url))] } },
});
