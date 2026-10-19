<script setup lang="ts">
// Before and after numbers as pairs of bars on one scale (smaller is better).
const props = defineProps<{
  unit: string
  rows: { label: string; before: number; after: number }[]
}>()

const max = Math.max(...props.rows.flatMap((r) => [r.before, r.after]))
const width = (v: number) => `${(v / max) * 100}%`
const fmt = (v: number) => `${v.toLocaleString('en-GB')} ${props.unit}`
</script>

<template>
  <div class="ub-bars" role="table" aria-label="Before and after">
    <div v-for="r in rows" :key="r.label" class="ub-bars__row" role="row">
      <div class="ub-bars__label" role="rowheader">{{ r.label }}</div>
      <div class="ub-bars__pair" role="cell">
        <div class="ub-bars__bar ub-bars__bar--before" :style="{ width: width(r.before) }">
          <span>Before: {{ fmt(r.before) }}</span>
        </div>
        <div class="ub-bars__bar ub-bars__bar--after" :style="{ width: width(r.after) }">
          <span>Now: {{ fmt(r.after) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>
