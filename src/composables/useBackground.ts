import { ref } from 'vue'

/**
 * 自定义背景：一张本机图片铺满主窗口底色区，卡片浮于其上。
 * 图片文件由 Rust 复制到 appData/backgrounds/（见 set_background_image），
 * 前端只持久化协议文件名与模糊度；设置页修改后通过模块级单例即时生效。
 * 播放页展开时被环境渐变层（App.vue z-15）覆盖，背景图自然让位。
 */

const FILE_KEY = 'lm.bgImage'
const BLUR_KEY = 'lm.bgBlur'

/** 背景图协议文件名（bg-<时间戳>.<ext>）；null = 未设置 */
const file = ref<string | null>(localStorage.getItem(FILE_KEY))
/** 背景模糊度（px，0 = 不模糊） */
const blur = ref(Number(localStorage.getItem(BLUR_KEY)) || 0)

function set(f: string | null, b = blur.value) {
  file.value = f
  blur.value = b
  if (f) localStorage.setItem(FILE_KEY, f)
  else localStorage.removeItem(FILE_KEY)
  localStorage.setItem(BLUR_KEY, String(b))
}

export function useBackground() {
  return { file, blur, set }
}
