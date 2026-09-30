import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { resolve } from "node:path";

const root = resolve(__dirname);

export default defineConfig({
  root,
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: resolve(root, "dist"),
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: {
      input: {
        index: resolve(root, "index.html"),
        settings: resolve(root, "settings.html"),
      },
    },
  },
  test: { include: ["src/**/*.test.ts"] },
});
