import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const devHost = process.env.MESH_DEV_HOST ?? "127.0.0.1";
const devPort = Number.parseInt(process.env.MESH_DEV_PORT ?? "1420", 10);

export default defineConfig({
  plugins: [svelte()],
  base: "./",
  server: {
    host: devHost,
    port: Number.isFinite(devPort) ? devPort : 1420,
    strictPort: true,
  },
  clearScreen: false,
});
