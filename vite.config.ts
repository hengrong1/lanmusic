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

  // CSS 目标浏览器（显式声明，避免 lightningcss 精简掉标准属性）：
  // style.css 里玻璃拟态同时写了「标准 backdrop-filter + -webkit- 前缀」——Chromium/WebView2
  // 只认标准属性，Safari(macOS WKWebView) 需要前缀。不声明 targets 时，lightningcss 压缩按
  // 其默认浏览器集判断「只留 -webkit- 即可」而删掉标准属性 → release 下 WebView2 里毛玻璃
  // 全部失效（背景图可见但卡片没有玻璃感）；dev 模式不压缩故无此问题。声明 targets 后
  // 两种写法都会被保留。（vite 8 为 rolldown 内核、不含 esbuild，故不能用 cssMinify: 'esbuild'）
  css: {
    lightningcss: {
      targets: {
        chrome: 105 << 16,
        edge: 105 << 16,
        safari: 15 << 16,
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
