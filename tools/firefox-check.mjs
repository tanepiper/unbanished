// Drive Firefox over WebDriver BiDi: open a page, collect its console (with stack traces for
// errors), click at given times, and save screenshots every 10 s. No downloads needed: Node 22+
// has WebSocket built in, and Firefox has the BiDi server built in.
//
// Start Firefox with a throwaway profile (your own profile is not touched):
//   mkdir -p /tmp/ff-profile
//   "/Applications/Firefox Nightly.app/Contents/MacOS/firefox" --profile /tmp/ff-profile \
//     --no-remote --remote-debugging-port 9333 about:blank &
// For WebGL instead of WebGPU, first:
//   echo 'user_pref("dom.webgpu.enabled", false);' > /tmp/ff-profile/user.js
//
// Then, with tools/dev-web.sh --app game --release running:
//   node tools/firefox-check.mjs 9333 "http://127.0.0.1:8733/?data=/game-data/" OUT 80 \
//     640,292@20 812,558@22          # New, then Ok: a new game
// Click coordinates are for a 1280x800 page and are shifted for other sizes (dialogs are
// centred). OUT gets console.txt, t5.png, t15.png, ... and final.png. Every 10 s it also logs
// the Web Audio state and output level (RMS) through an analyser on the destination; the
// context stays suspended until the first click in the game (browser autoplay rules).
import { writeFileSync, mkdirSync } from 'node:fs'

const [port, url, out, secs, ...clicks] = process.argv.slice(2)
mkdirSync(out, { recursive: true })
let ws
for (let i = 0; i < 60; i++) {
  try {
    ws = new WebSocket(`ws://127.0.0.1:${port}/session`)
    await new Promise((ok, bad) => { ws.onopen = ok; ws.onerror = bad })
    break
  } catch { ws = null; await new Promise((r) => setTimeout(r, 500)) }
}
if (!ws) throw new Error('could not connect to Firefox')
let id = 0
const pending = new Map()
const logs = []
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data)
  if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); return }
  if (msg.method === 'log.entryAdded') {
    const p = msg.params
    const line = `[${p.level}] ${p.text ?? ''}${p.stackTrace && p.level === 'error' ? '\n   ' + (p.stackTrace.callFrames ?? []).map((f) => f.functionName).join('\n   ') : ''}`
    logs.push(line)
    console.log(line.slice(0, 400))
  }
}
const send = (method, params = {}) =>
  new Promise((ok) => { const i = ++id; pending.set(i, ok); ws.send(JSON.stringify({ id: i, method, params })) })

const s = await send('session.new', { capabilities: {} })
if (s.error) console.log('session.new', s.error, s.message)
await send('session.subscribe', { events: ['log.entryAdded'] })
const tree = await send('browsingContext.getTree', {})
const ctx = tree.result.contexts[0].context
await send('browsingContext.setViewport', { context: ctx, viewport: { width: 1280, height: 800 } })
// Audio probe: every AudioNode connected to a context's destination is also fed to an analyser,
// so `__audioProbe()` reports the context state and the output level.
await send('script.addPreloadScript', {
  functionDeclaration: `() => {
    const probes = []
    const connect = AudioNode.prototype.connect
    AudioNode.prototype.connect = function (dest, ...rest) {
      const r = connect.call(this, dest, ...rest)
      if (dest instanceof AudioDestinationNode) {
        const a = this.context.createAnalyser()
        a.fftSize = 2048
        connect.call(this, a)
        probes.push(a)
      }
      return r
    }
    window.__audioProbe = () => probes.map((a) => {
      const d = new Float32Array(a.fftSize)
      a.getFloatTimeDomainData(d)
      let s = 0
      for (const v of d) s += v * v
      return { state: a.context.state, time: +a.context.currentTime.toFixed(1), rms: +Math.sqrt(s / d.length).toFixed(5) }
    })
  }`,
})
await send('browsingContext.navigate', { context: ctx, url, wait: 'complete' })
const start = Date.now()
const shot = async (name) => {
  const r = await send('browsingContext.captureScreenshot', { context: ctx })
  if (r.result) writeFileSync(`${out}/${name}.png`, Buffer.from(r.result.data, 'base64'))
}
const evaluate = async (expr) => {
  const r = await send('script.evaluate', { expression: expr, target: { context: ctx }, awaitPromise: true })
  return r.result?.result?.value ?? r.error ?? r.result
}
const plan = clicks.map((c) => { const [xy, t] = c.split('@'); const [x, y] = xy.split(',').map(Number); return { x, y, t: Number(t), done: false } })
let next = 5
while ((Date.now() - start) / 1000 < Number(secs)) {
  const t = (Date.now() - start) / 1000
  for (const c of plan) {
    if (!c.done && t >= c.t) {
      c.done = true
      // Coordinates are for a 1280x800 page; the dialogs are centred, so shift by half the difference.
      const size = JSON.parse(await evaluate('JSON.stringify([innerWidth, innerHeight])'))
      const x = Math.round(c.x + (size[0] - 1280) / 2), y = Math.round(c.y + (size[1] - 800) / 2)
      await send('input.performActions', { context: ctx, actions: [{ type: 'pointer', id: 'm', parameters: { pointerType: 'mouse' }, actions: [{ type: 'pointerMove', x, y }, { type: 'pointerDown', button: 0 }, { type: 'pointerUp', button: 0 }] }] })
      console.log(`clicked ${x},${y} at ${t.toFixed(1)}s`)
    }
  }
  if (t >= next) {
    await shot(`t${Math.round(t)}`)
    console.log(`audio t=${Math.round(t)}`, await evaluate('JSON.stringify(window.__audioProbe ? __audioProbe() : null)'))
    next += 10
  }
  await new Promise((r) => setTimeout(r, 250))
}
await shot('final')
const info = await evaluate(`JSON.stringify({ ua: navigator.userAgent, gpu: !!navigator.gpu, status: document.body.innerText.slice(0, 300) })`)
console.log('INFO', info)
writeFileSync(`${out}/console.txt`, logs.join('\n'))
await send('session.end', {})
ws.close()
process.exit(0)
