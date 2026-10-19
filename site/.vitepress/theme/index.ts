import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import { h } from 'vue'
import PlayGame from './components/PlayGame.vue'
import Gallery from './components/Gallery.vue'
import HeroVideo from './components/HeroVideo.vue'
import WorkLoop from './components/WorkLoop.vue'
import ReleaseHero from './components/ReleaseHero.vue'
import Shot from './components/Shot.vue'
import BeforeAfter from './components/BeforeAfter.vue'
import PostList from './components/PostList.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  // The home page's hero shows the montage video in place of an image.
  Layout: () => h(DefaultTheme.Layout, null, { 'home-hero-image': () => h(HeroVideo) }),
  enhanceApp({ app }) {
    app.component('PlayGame', PlayGame)
    app.component('Gallery', Gallery)
    app.component('WorkLoop', WorkLoop)
    app.component('ReleaseHero', ReleaseHero)
    app.component('Shot', Shot)
    app.component('BeforeAfter', BeforeAfter)
    app.component('PostList', PostList)
  },
} satisfies Theme
