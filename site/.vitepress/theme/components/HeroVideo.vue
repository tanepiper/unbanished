<script setup lang="ts">
// The home page's hero picture: a 39-second montage that Un-Banished recorded itself, without a
// window (tools/videos.sh montage). It plays muted and in a loop; the poster shows until it loads.
import { withBase } from 'vitepress'
</script>

<template>
  <video
    class="ub-hero-video"
    :src="withBase('/assets/videos/montage.webm')"
    :poster="withBase('/assets/videos/montage-poster.webp')"
    autoplay
    loop
    muted
    playsinline
    preload="metadata"
    aria-label="Un-Banished in motion: the camera descends over a river town, citizens trade at a market, farmers work autumn fields, a winter town, a year of seasons, and the camera rises again"
  ></video>
</template>
