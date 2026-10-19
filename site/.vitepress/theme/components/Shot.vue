<script setup lang="ts">
// A picture or a looping video in a post, with a caption. Videos play muted, like a GIF; the
// poster shows until they load.
import { withBase } from 'vitepress'

defineProps<{
  src: string
  alt: string
  caption?: string
  poster?: string
}>()
</script>

<template>
  <figure class="ub-shot">
    <video
      v-if="src.endsWith('.webm')"
      :src="withBase(src)"
      :poster="poster ? withBase(poster) : undefined"
      autoplay
      loop
      muted
      playsinline
      preload="none"
      :aria-label="alt"
    ></video>
    <a v-else :href="withBase(src)" target="_blank" rel="noopener">
      <img :src="withBase(src)" :alt="alt" loading="lazy" />
    </a>
    <figcaption v-if="caption">{{ caption }}</figcaption>
  </figure>
</template>
