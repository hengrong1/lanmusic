import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },

  // 注：CSS 压缩（lightningcss）会把 style.css 里「标准 + -webkit- 前缀」的同值双写精简成
  // 只留前缀，Chromium/WebView2 不认该前缀 → 毛玻璃失效。已确认该精简无法通过 targets /
  // browserslist / 关闭 minify 规避（发生在 Tailwind 的 CSS 处理阶段），故玻璃声明改用
  // CSS 变量间接引用（见 style.css 的 --glass-* 变量），压缩器无法判定等价即原样保留。

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
