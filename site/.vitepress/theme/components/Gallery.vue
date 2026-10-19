<script setup lang="ts">
import { withBase } from 'vitepress'

const shots = [
  { src: '/assets/screenshots/main-menu.webp', alt: 'The main menu over one of its 3D town scenes', caption: 'The main menu' },
  { src: '/assets/screenshots/new-game.webp', alt: 'The settlers next to their first stockpile', caption: 'The first minute of a new town' },
  { src: '/assets/screenshots/town-overview.webp', alt: 'A town of houses, fields and an orchard in a forest', caption: 'A town loaded from a save' },
  { src: '/assets/screenshots/town-hall.webp', alt: 'The Town Hall window open on its Inventory tab', caption: 'The Town Hall window' },
]
</script>

<template>
  <div class="gallery">
    <figure v-for="s in shots" :key="s.src">
      <a :href="withBase(s.src)" target="_blank" rel="noopener">
        <img :src="withBase(s.src)" :alt="s.alt" loading="lazy" />
      </a>
      <figcaption>{{ s.caption }}</figcaption>
    </figure>
  </div>
</template>

<style scoped>
.gallery {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 20px;
  margin-top: 24px;
}
figure {
  margin: 0;
}
img {
  display: block;
  width: 100%;
  aspect-ratio: 16 / 9;
  object-fit: cover;
  border-radius: 10px;
  border: 1px solid var(--vp-c-divider);
  transition: transform 0.2s ease, border-color 0.2s ease;
}
a:hover img {
  transform: translateY(-2px);
  border-color: var(--vp-c-brand-2);
}
figcaption {
  margin-top: 8px;
  font-size: 14px;
  color: var(--vp-c-text-2);
  text-align: center;
}
</style>
