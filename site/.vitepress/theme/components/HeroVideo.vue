<script setup lang="ts">
// The home page's hero picture: a 64-second montage that Un-Banished recorded itself, without a
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
    aria-label="Un-Banished in motion: a flight down a river, the trader's boat, a fisher on a dock, the market, a farmer, a vendor and a labourer at work, towns in autumn and winter, a year of seasons, and the camera rising over a town"
  ></video>
</template>
