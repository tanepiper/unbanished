import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import PlayGame from './components/PlayGame.vue'
import Gallery from './components/Gallery.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('PlayGame', PlayGame)
    app.component('Gallery', Gallery)
  },
} satisfies Theme
