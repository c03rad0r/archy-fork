import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://localhost:1337",
        changeOrigin: true,
      },
      "/healthz": "http://localhost:1337",
    },
  },
  build: {
    outDir: "dist",
    sourcemap: true,
    target: "es2022",
  },
});
