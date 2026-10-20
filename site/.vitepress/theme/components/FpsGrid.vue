<script setup lang="ts">
// Frame rates as a grid of tiles: a row per town, a column per game speed. A tile is green at
// 59 frames a second or more (the target is 60), amber from 45, red below. Its bar fills at 60.
const props = withDefaults(
  defineProps<{
    caption?: string
    columns?: string[]
    rows: { town: string; detail?: string; cells: { fps: number; note?: string }[] }[]
  }>(),
  { columns: () => ['Normal speed', '5× speed', '10× speed'] },
)

const level = (fps: number) => (fps >= 59 ? 'good' : fps >= 45 ? 'near' : 'low')
const word = { good: 'on target', near: 'almost', low: 'below 60' }
const width = (fps: number) => `${Math.min(100, (fps / 60) * 100)}%`
</script>

<template>
  <figure class="ub-fps">
    <div class="ub-fps__grid" role="table" :aria-label="caption ?? 'Frames a second'">
      <div class="ub-fps__row ub-fps__head" role="row">
        <div role="columnheader">Town</div>
        <div v-for="c in props.columns" :key="c" role="columnheader">{{ c }}</div>
      </div>
      <div v-for="r in rows" :key="r.town" class="ub-fps__row" role="row">
        <div class="ub-fps__town" role="rowheader">
          <strong>{{ r.town }}</strong>
          <span v-if="r.detail">{{ r.detail }}</span>
        </div>
        <div
          v-for="(c, i) in r.cells"
          :key="i"
          :class="['ub-fps__tile', `ub-fps__tile--${level(c.fps)}`]"
          role="cell"
        >
          <span class="ub-fps__label">{{ props.columns[i] }}</span>
          <span class="ub-fps__fps">{{ Math.round(c.fps) }}</span>
          <span class="ub-fps__word">fps · {{ word[level(c.fps)] }}</span>
          <span class="ub-fps__bar"><span :style="{ width: width(c.fps) }"></span></span>
          <span v-if="c.note" class="ub-fps__note">{{ c.note }}</span>
        </div>
      </div>
    </div>
    <figcaption v-if="caption">{{ caption }}</figcaption>
  </figure>
</template>
