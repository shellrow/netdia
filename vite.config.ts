import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from '@tailwindcss/vite';

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    vue(),
    tailwindcss()
  ],
  resolve: {
    alias: {
      "@": new URL("./src", import.meta.url).pathname,
    },
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return;
          if (id.includes("chart.js")) return "chart";
          if (id.includes("primevue")) {
            if (/primevue\/(datatable|column|paginator)\//.test(id)) {
              return "primevue-data";
            }
            if (/primevue\/(dialog|drawer|tooltip|styleclass)\//.test(id)) {
              return "primevue-overlay";
            }
            if (/primevue\/(chart|timeline)\//.test(id)) {
              return "primevue-visualization";
            }
            return "primevue-controls";
          }
          if (id.includes("@primeuix")) return "primeuix";
          if (id.includes("@tauri-apps")) return "tauri";
          if (id.includes("/vue/") || id.includes("vue-router")) return "vue";
          return "vendor";
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
