<script setup lang="ts">
// The work loop on the home page, as HTML rather than one picture, so that it reflows on narrow
// screens: four columns, then two, then one. docs/assets/process-loop.svg is the same loop as one
// image, for the documents on GitHub. The icons are decoration (the text says the same), so their
// alt text is empty.
import { withBase } from 'vitepress'

const steps = [
  { icon: 'read', title: 'Read the data files', text: 'The .pkg packages and their formats' },
  { icon: 'decompile', title: 'Decompile', text: 'Ghidra turns machine code into pseudocode' },
  { icon: 'trace', title: 'Trace', text: 'Understand one function and write it down in plain words' },
  { icon: 'measure', title: 'Measure', text: 'Run the original function in an emulator' },
  { icon: 'rebuild', title: 'Rebuild in Rust', text: 'New code from the notes, citing the original function' },
  { icon: 'check', title: 'Check', text: 'Tests against the emulator, saves and screenshots' },
  { icon: 'commit', title: 'Write down, commit', text: 'Documents, the checklist, and one commit per piece' },
]
</script>

<template>
  <div class="ub-loop">
    <ol>
      <li v-for="(s, i) in steps" :key="s.icon" :class="{ key: s.icon === 'trace' }">
        <img :src="withBase(`/assets/loop/${s.icon}.svg`)" alt="" width="28" height="28" />
        <div>
          <p class="title"><span class="n">{{ i + 1 }}</span>{{ s.title }}</p>
          <p class="text">{{ s.text }}</p>
        </div>
      </li>
      <li class="back" aria-label="If a check fails, the work goes back to step 3">
        <span class="arrow" aria-hidden="true">↺</span>
        <div>
          <p class="title">Does not match?</p>
          <p class="text">The work goes back to step 3, Trace, until the check passes.</p>
        </div>
      </li>
    </ol>
    <p class="note">Every part of the game went through this loop.</p>
  </div>
</template>

<style scoped>
.ub-loop {
  margin: 24px 0;
}
ol {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 14px;
}
li {
  margin: 0;
  display: flex;
  gap: 12px;
  align-items: flex-start;
  padding: 16px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 10px;
  background: var(--vp-c-bg-soft);
}
li.key {
  border-color: var(--vp-c-brand-1);
}
li.back {
  border-style: dashed;
  background: transparent;
}
img {
  flex: none;
  width: 28px;
  height: 28px;
  margin-top: 1px;
}
.arrow {
  flex: none;
  width: 28px;
  font-size: 24px;
  line-height: 28px;
  text-align: center;
  color: var(--vp-c-brand-1);
}
/* More specific than the theme's `.vp-doc p` and `.vp-doc li + li`. */
.ub-loop p {
  margin: 0;
}
.ub-loop li + li {
  margin-top: 0;
}
.title {
  font-weight: 600;
  line-height: 1.4;
  color: var(--vp-c-text-1);
}
.n {
  color: var(--vp-c-brand-1);
  font-weight: 700;
  margin-right: 8px;
}
.ub-loop .text {
  margin-top: 4px;
  font-size: 14px;
  line-height: 1.5;
  color: var(--vp-c-text-2);
}
.ub-loop .note {
  margin-top: 14px;
  font-size: 14px;
  color: var(--vp-c-text-2);
}
@media (max-width: 960px) {
  ol {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
@media (max-width: 560px) {
  ol {
    grid-template-columns: 1fr;
  }
}
</style>
