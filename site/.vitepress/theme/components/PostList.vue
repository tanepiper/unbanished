<script setup lang="ts">
// The blog's front page: a card for every post (releases and articles), newest first.
import { withBase } from 'vitepress'
import { data as posts } from '../../../blog/posts.data'

const when = (date: string) =>
  new Date(date + 'T12:00:00Z').toLocaleDateString('en-GB', { day: 'numeric', month: 'long', year: 'numeric' })
</script>

<template>
  <div class="ub-posts">
    <a v-for="p in posts" :key="p.url" class="ub-post-card" :href="withBase(p.url)">
      <img :src="withBase(p.cover)" :alt="''" loading="lazy" />
      <div class="ub-post-card__text">
        <p class="ub-post-card__meta">
          {{ p.version ? `Un-Banished ${p.version}` : p.kind ?? 'Article' }} · {{ when(p.date) }}
        </p>
        <p class="ub-post-card__name">{{ p.name }}</p>
        <p class="ub-post-card__summary">{{ p.summary }}</p>
      </div>
    </a>
  </div>
</template>
