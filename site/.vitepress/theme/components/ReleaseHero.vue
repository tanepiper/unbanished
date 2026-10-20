<script setup lang="ts">
// The top of a post: a wide video (or picture) with the post's name over it, and the release (or,
// for an article, its kind) and the date.
import { withBase } from 'vitepress'

const props = defineProps<{
  name: string
  version?: string
  /** An article's kind, shown in place of a version ("Behind the scenes"). */
  kind?: string
  /** YYYY-MM-DD, or a date and time. */
  date: string
  video?: string
  poster?: string
  image?: string
  alt: string
}>()

const day = props.date.length === 10 ? `${props.date}T12:00:00Z` : props.date
const when = new Date(day).toLocaleDateString('en-GB', {
  day: 'numeric',
  month: 'long',
  year: 'numeric',
  timeZone: 'UTC',
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
      <p class="ub-release-hero__version">
        {{ version ? `Un-Banished ${version}` : kind ?? 'Article' }} · {{ when }}
      </p>
      <h1 class="ub-release-hero__name">{{ name }}</h1>
    </div>
  </div>
</template>
