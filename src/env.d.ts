/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<object, object, unknown>
  export default component
}

import type { MessageSchema } from './i18n/locales/zh'

declare module 'vue' {
  interface ComponentCustomProperties {
    $t: (key: string) => string
  }
}

declare module '@vue/runtime-core' {
  interface ComponentCustomProperties {
    $t: (key: string) => string
  }
}