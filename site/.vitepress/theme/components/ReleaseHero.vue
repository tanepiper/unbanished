<script setup lang="ts">
// The top of a release post: a wide video (or picture) with the release's name over it.
import { withBase } from 'vitepress'

const props = defineProps<{
  name: string
  version: string
  date: string
  video?: string
  poster?: string
  image?: string
  alt: string
}>()

const when = new Date(props.date + 'T12:00:00Z').toLocaleDateString('en-GB', {
  day: 'numeric',
  month: 'long',
  year: 'numeric',
})
</script>

<template>
  <div class="ub-release-hero">
    <video
      v-if="video"
      :src="withBase(video)"
      :poster="poster ? withBase(poster) : undefined"
      autoplay
      loop
      muted
      playsinline
      preload="metadata"
      :aria-label="alt"
    ></video>
    <img v-else-if="image" :src="withBase(image)" :alt="alt" />
    <div class="ub-release-hero__text">
      <p class="ub-release-hero__version">Un-Banished {{ version }} · {{ when }}</p>
      <h1 class="ub-release-hero__name">{{ name }}</h1>
    </div>
  </div>
</template>
