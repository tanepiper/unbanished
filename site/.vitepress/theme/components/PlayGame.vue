<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { withBase } from 'vitepress'

const gameUrl = withBase('/game/index.html')
const started = ref(false)
const touchOnly = ref(false)
const frame = ref<HTMLIFrameElement | null>(null)

onMounted(() => {
  touchOnly.value = window.matchMedia('(pointer: coarse)').matches && !window.matchMedia('(pointer: fine)').matches
})

function start() {
  started.value = true
}

function stop() {
  started.value = false
}

function fullscreen() {
  frame.value?.requestFullscreen?.()
  frame.value?.focus()
}
</script>

<template>
  <div class="play">
    <div v-if="!started" class="intro">
      <img class="emblem" :src="withBase('/assets/un-banished-icon.svg')" alt="" width="120" height="120" />
      <div class="text">
        <h2>Play Un-Banished in your browser</h2>
        <p>
          You need the four game files from your own copy of Banished:
          <code>data0.pkg</code>, <code>data1.pkg</code>, <code>patch0.pkg</code> and <code>patch1.pkg</code>.
          They are in the game's <code>WinData</code> folder.
          <a :href="withBase('/guide/#4-find-your-game-data')">Where is this folder?</a>
        </p>
        <ul>
          <li><strong>Your files stay on your computer.</strong> The page reads them in your browser. It does not upload them.</li>
          <li><strong>Your saves stay in this browser.</strong> If you clear this site's data, you delete them.</li>
          <li>Use a desktop browser with a keyboard and a mouse. We test in Chrome and Firefox. Edge should also work.</li>
          <li>The game is about 8 MB. It downloads only when you press Start.</li>
        </ul>
        <p v-if="touchOnly" class="warn">
          This device seems to have a touch screen only. Banished needs a keyboard and a mouse, so the game may be hard to
          control here.
        </p>
        <div class="actions">
          <button class="primary" type="button" @click="start">Start the game</button>
          <a class="secondary" :href="gameUrl" target="_blank" rel="noopener">Open in its own tab</a>
        </div>
      </div>
    </div>

    <div v-else class="stage">
      <iframe
        ref="frame"
        :src="gameUrl"
        title="Un-Banished"
        allow="fullscreen; autoplay; gamepad"
        allowfullscreen
      ></iframe>
      <div class="bar">
        <span class="hint">Press <strong>Choose files</strong> at the top left and select all four <code>.pkg</code> files. Then click the game once.</span>
        <span class="buttons">
          <button type="button" @click="fullscreen">Full screen</button>
          <a :href="gameUrl" target="_blank" rel="noopener">Own tab</a>
          <button type="button" @click="stop">Close</button>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.play {
  margin: 32px 0 16px;
}
.intro {
  display: flex;
  gap: 32px;
  align-items: flex-start;
  padding: 32px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 16px;
  background:
    radial-gradient(circle at 12% 20%, rgba(243, 179, 86, 0.12), transparent 45%),
    var(--vp-c-bg-soft);
}
.emblem {
  flex: none;
}
.text h2 {
  margin: 0 0 12px;
  padding: 0;
  border: 0;
  font-family: var(--vp-font-family-heading);
  font-size: 28px;
}
.text p,
.text li {
  color: var(--vp-c-text-2);
  line-height: 1.65;
}
.text ul {
  margin: 12px 0 0;
  padding-left: 20px;
}
.warn {
  margin-top: 12px;
  padding: 10px 14px;
  border-radius: 8px;
  background: var(--vp-c-warning-soft);
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 24px;
}
.actions .primary {
  padding: 10px 22px;
  border-radius: 22px;
  font-weight: 600;
  color: var(--vp-button-brand-text);
  background: var(--vp-button-brand-bg);
}
.actions .primary:hover {
  background: var(--vp-button-brand-hover-bg);
}
.actions .secondary {
  padding: 10px 22px;
  border-radius: 22px;
  font-weight: 600;
  border: 1px solid var(--vp-c-divider);
  color: var(--vp-c-text-1);
  text-decoration: none;
}
.stage iframe {
  display: block;
  width: 100%;
  aspect-ratio: 16 / 10;
  max-height: 78vh;
  border: 1px solid var(--vp-c-brand-3);
  border-radius: 12px;
  background: #000;
}
/* Full screen: the whole screen, whatever the browser's own full-screen styles do with the
   frame's aspect ratio and height limit (issue #14). */
.stage iframe:fullscreen {
  width: 100vw;
  height: 100vh;
  max-height: none;
  aspect-ratio: auto;
  border: 0;
  border-radius: 0;
}
.bar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  justify-content: space-between;
  align-items: center;
  margin-top: 10px;
  font-size: 14px;
  color: var(--vp-c-text-2);
}
.buttons {
  display: flex;
  gap: 8px;
}
.buttons button,
.buttons a {
  padding: 4px 14px;
  border-radius: 14px;
  border: 1px solid var(--vp-c-divider);
  color: var(--vp-c-text-1);
  text-decoration: none;
  font-weight: 500;
}
.buttons button:hover,
.buttons a:hover {
  border-color: var(--vp-c-brand-2);
}
@media (max-width: 640px) {
  .intro {
    flex-direction: column;
    padding: 20px;
  }
}
</style>
