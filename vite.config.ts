import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;
const isVitest = process.env.VITEST === "true";

export default defineConfig(async () => ({
  plugins: [sveltekit()],
  clearScreen: false,
  // Component tests must resolve Svelte's browser entry so
  // @testing-library/svelte can mount into jsdom. Omit `resolve`
  // entirely otherwise so normal dev/SSR defaults remain intact.
  ...(isVitest ? { resolve: { conditions: ["browser"] } } : {}),
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.{test,spec}.{js,ts}"],
  },
}));
