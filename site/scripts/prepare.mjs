// Builds the site's generated pages from the repository's own documents, so the site and the
// repository say the same thing. Run before `vitepress build` and `vitepress dev`.
//
// Hand-written (tracked): index.md, play.md, about/index.md. Every other page is generated from
// a file in the repository (see PAGES and GUIDE below) and is git-ignored. Images and videos are
// copied from docs/assets/ to public/assets/.
import { cpSync, mkdirSync, readFileSync, writeFileSync, existsSync, statSync } from 'node:fs'
import { dirname, join, normalize, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const site = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const repo = resolve(site, '..')
const GITHUB = 'https://github.com/tanepiper/unbanished'
// Links to the published site in the repository's files become links inside the site.
const SITE = 'https://tanepiper.github.io/unbanished/'

// The user guide is one file in the repository. On the site, its `##` sections are split into
// pages: each page lists the headings of the sections it takes, in order.
const GUIDE_FILE = 'docs/USER-GUIDE.md'
const GUIDE = [
  { path: '/guide/', title: 'Setup guide', description: 'Install Un-Banished and start playing.', intro: true,
    sections: ['1. Check what you need', '2. Install the build tools', '3. Download and build Un-Banished', '4. Find your game data', '5. Play'] },
  { path: '/guide/browser', title: 'The browser version', description: 'Play in a web browser, or run the browser version yourself.',
    sections: ['Play in the browser'] },
  { path: '/guide/controls', title: 'Controls', description: 'The default keys and mouse actions.',
    sections: ['Controls'] },
  { path: '/guide/saves', title: 'Saves and settings', description: 'Where Un-Banished keeps saves and settings.',
    sections: ['Saves', 'Settings'] },
  { path: '/guide/mods', title: 'Mods', description: 'Use BanishedKit mods, and get them from Nexus Mods if you like.',
    sections: ['Mods'] },
  // The guide's short Multiplayer section: docs/guide/MULTIPLAYER.md (in PAGES) is written to the
  // same path afterwards and replaces it, so links to the section open the full page.
  { path: '/guide/multiplayer', title: 'Multiplayer', description: 'Build one town together.',
    sections: ['Multiplayer'] },
  { path: '/guide/command-line', title: 'Command line', description: 'Play, take pictures, ask the game questions, record games, mods and the relay, from the command line.',
    sections: ['Command-line options'] },
  { path: '/guide/troubleshooting', title: 'Troubleshooting', description: 'What to do when something goes wrong, your version, and how to report a difference.',
    sections: ['Troubleshooting', 'Version', 'Reporting a difference from the original'] },
]

// Repository files that have a page of their own on the site.
const PAGES = {
  'README.md': { path: '/', site: false },
  'CHANGELOG.md': { path: '/changelog', title: 'Changelog', description: 'Every notable change, by version.' },
  'docs/PROCESS.md': { path: '/about/how-it-was-made', title: 'How it was made', description: 'How Ghidra, REA, an AI agent and a human rebuilt Banished.' },
  'docs/guide/PLAYING.md': { path: '/guide/playing', description: 'The first steps in a new town, with pictures and short videos.' },
  'docs/guide/MULTIPLAYER.md': { path: '/guide/multiplayer', description: 'Build one town together: the relay, hosting, joining, and what is shared.' },

  'docs/develop/README.md': { path: '/develop/', description: 'Where to start as a developer.' },
  'docs/develop/BUILDING.md': { path: '/develop/building', description: 'Build the game, the viewer and the browser version.' },
  'docs/develop/TESTING.md': { path: '/develop/testing', description: 'The test suites, the checks before a commit, and headless captures.' },
  'docs/develop/PROBE.md': { path: '/develop/probe', title: 'The probe', description: 'Look at and drive a running game: queries for the UI and the town, in the console and in tests.' },
  'docs/COMPATIBILITY.md': { path: '/develop/compatibility', description: 'The tools that prove two versions play alike.' },
  'docs/PERFORMANCE.md': { path: '/develop/performance', description: 'How to measure and profile Un-Banished.' },
  'AGENTS.md': { path: '/develop/contributing', title: 'Contributing', description: 'The project rules for people and AI agents.' },
  'docs/develop/VISUAL-CHANGES.md': { path: '/develop/visual-changes', description: 'Prove that a change to the picture changes only what it should.' },
  'docs/develop/VIDEOS.md': { path: '/develop/videos', description: 'Record videos of the game without a window.' },
  'docs/develop/API.md': { path: '/develop/api', description: 'The Rust API documentation of the libraries.' },

  'docs/develop/guides/README.md': { path: '/develop/guides/', description: 'From the original program to tested Rust code, step by step.' },
  'docs/develop/guides/TRACE.md': { path: '/develop/guides/trace', description: 'Find and understand a function in the original program.' },
  'docs/develop/guides/EMULATE.md': { path: '/develop/guides/emulate', description: 'Run original functions in an emulator to get exact numbers.' },
  'docs/develop/guides/FORMAT.md': { path: '/develop/guides/format', description: 'Decode a resource or save block and write a reader for it.' },
  'docs/develop/guides/PORT.md': { path: '/develop/guides/port', description: 'Turn a traced behaviour into Rust code.' },
  'docs/develop/guides/SHADER.md': { path: '/develop/guides/shader', description: 'Port one of the game’s shaders to WGSL.' },
  'docs/develop/guides/VERIFY.md': { path: '/develop/guides/verify', description: 'Prove that a change works.' },
  'docs/develop/guides/LAND.md': { path: '/develop/guides/land', description: 'Document, commit and push a change.' },

  'docs/topics/README.md': { path: '/topics/', description: 'How the original game works, and how Un-Banished rebuilds it.' },
  'docs/topics/GAME-DATA.md': { path: '/topics/game-data', description: 'Packages, compiled resources and type ids.' },
  'docs/topics/TERRAIN.md': { path: '/topics/terrain', description: 'How a map is made from a seed.' },
  'docs/topics/SAVES.md': { path: '/topics/saves', description: 'What a save file contains.' },
  'docs/topics/CITIZENS.md': { path: '/topics/citizens', description: 'How citizens decide what to do.' },
  'docs/topics/EXACT-MATHS.md': { path: '/topics/exact-maths', description: 'Random numbers, floats and the game clock.' },
  'docs/topics/GAME-LOOP.md': { path: '/topics/game-loop', description: 'Engine loops, the game loop and the simulation step.' },
  'docs/topics/EMULATOR.md': { path: '/topics/emulator', description: 'Running the original program’s functions under Unicorn.' },
  'docs/topics/AUDIO.md': { path: '/topics/audio', description: 'Sound files, the WMA decoder and the mixer.' },
  'docs/topics/INTERFACE.md': { path: '/topics/interface', description: 'Dialog resources, layout and controls.' },

  'docs/develop/CRATES.md': { path: '/reference/', description: 'Every app and library in the workspace.' },
  'apps/game/README.md': { path: '/reference/game', description: 'The game program, unbanished.' },
  'apps/viewer/README.md': { path: '/reference/viewer', description: 'The asset browser, unbanished-viewer.' },
  'apps/cli/README.md': { path: '/reference/cli', description: 'Command-line tools, unbanished-cli.' },
  'apps/modmanager/README.md': { path: '/reference/modmanager', description: 'The mod manager, unbanished-mods.' },
  'apps/relay/README.md': { path: '/reference/relay', description: 'The multiplayer relay, unbanished-relay.' },
  'crates/core/README.md': { path: '/reference/core', description: 'unbanished-core: exact maths, Random, noise, game time.' },
  'crates/terrain/README.md': { path: '/reference/terrain', description: 'unbanished-terrain: map generation and ground queries.' },
  'crates/path/README.md': { path: '/reference/path', description: 'unbanished-path: the A* pathfinder and the sector graph.' },
  'crates/game/README.md': { path: '/reference/game-rules', description: 'unbanished-game: the game rules.' },
  'crates/formats/README.md': { path: '/reference/formats', description: 'unbanished-formats: readers for the game’s files.' },
  'crates/ui/README.md': { path: '/reference/ui', description: 'unbanished-ui: the game’s interface system.' },
  'crates/gfx/README.md': { path: '/reference/gfx', description: 'unbanished-gfx: shared wgpu rendering.' },
  'crates/audio/README.md': { path: '/reference/audio', description: 'unbanished-audio: sound decoding and playback.' },
  'crates/wma/README.md': { path: '/reference/wma', description: 'unbanished-wma: a WMA v2 decoder.' },
  'crates/mods/README.md': { path: '/reference/mods', description: 'unbanished-mods: the mod list shared by the game and the mod manager.' },
  'crates/banishedinfo/README.md': { path: '/reference/banishedinfo', description: 'unbanished-banishedinfo: optional banishedinfo.com support.' },
  'crates/nexus/README.md': { path: '/reference/nexus', description: 'unbanished-nexus: optional Nexus Mods support.' },
  'crates/net/README.md': { path: '/reference/net', description: 'unbanished-net: the multiplayer relay protocol and connection.' },
}

/** VitePress's anchor for a heading anchor in GitHub style (a leading digit gets `_`). */
const siteAnchor = (a) => (/^\d/.test(a) ? `_${a}` : a)

/** GitHub's anchor for a heading. */
const slug = (heading) =>
  heading
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .replace(/\s/g, '-')

// The guide page of each heading anchor in the user guide.
const guideAnchors = new Map()
for (const p of GUIDE) for (const h of p.sections) guideAnchors.set(slug(h), p.path)

/** Rewrite one link target found in `from` (a repository-relative file). */
function target(href, from) {
  if (href.startsWith(SITE)) return '/' + href.slice(SITE.length)
  if (/^(https?:|mailto:|\/)/.test(href)) return href
  if (href.startsWith('#')) {
    if (from === GUIDE_FILE) return guideTarget(href.slice(1))
    return '#' + siteAnchor(href.slice(1))
  }
  const [path, anchor] = href.split('#')
  const file = normalize(join(dirname(from), path)).replace(/\\/g, '/').replace(/\/$/, '')
  if (file === GUIDE_FILE) return anchor ? guideTarget(anchor) : '/guide/'
  if (file.startsWith('docs/assets/')) return '/' + file.slice('docs/'.length)
  const key = PAGES[file] ? file : `${file}/README.md`
  const page = existsSync(join(repo, key)) ? PAGES[key] : undefined
  if (page && page.site !== false) return page.path + (anchor ? `#${siteAnchor(anchor)}` : '')
  if (page) return page.path
  const kind = existsSync(join(repo, file)) && statSync(join(repo, file)).isDirectory() ? 'tree' : 'blob'
  return `${GITHUB}/${kind}/main/${file}${anchor ? `#${anchor}` : ''}`
}

/** A link to a heading of the user guide: the page that holds it, and the heading on it. */
function guideTarget(anchor) {
  const page = guideAnchors.get(anchor)
  if (page) return page
  // A heading inside a section: find the section that contains it.
  const md = read(GUIDE_FILE)
  let current = '/guide/'
  for (const line of md.split('\n')) {
    const h2 = /^## (.*)$/.exec(line)
    if (h2) current = guideAnchors.get(slug(h2[1])) ?? current
    const h = /^#{3,4} (.*)$/.exec(line)
    if (h && slug(h[1]) === anchor) return `${current}#${siteAnchor(anchor)}`
  }
  return `/guide/#${siteAnchor(anchor)}`
}

/** Rewrite markdown links and images, and HTML src/href attributes, outside code blocks. */
function rewrite(md, from) {
  return md
    .split(/(^```[\s\S]*?^```)/m)
    .map((part, i) =>
      i % 2
        ? part
        : part
            .replace(/(\]\()([^)\s]+)(\))/g, (_, a, href, b) => a + target(href, from) + b)
            .replace(/\b(src|href|poster)="([^"]+)"/g, (_, attr, href) => `${attr}="${target(href, from)}"`),
    )
    .join('')
}

/** The `## heading` sections of a markdown file, in order, with the text before the first. */
function sections(md) {
  const parts = md.split(/^## /m)
  const out = { intro: parts[0], list: [] }
  for (const p of parts.slice(1)) {
    const nl = p.indexOf('\n')
    out.list.push({ heading: p.slice(0, nl).trim(), body: p.slice(nl + 1).replace(/\n---\s*$/, '').trim() })
  }
  return out
}

/** The body of one `## heading` section of a markdown file. */
function section(md, heading) {
  const s = sections(md).list.find((s) => s.heading === heading)
  if (!s) throw new Error(`section not found: ${heading}`)
  return s.body
}

/** Drop the centred logo paragraph that opens the repository's documents. */
function dropLogo(md) {
  return md.replace(/^<p align="center"><img src="[^"]*un-banished-logo\.svg"[^>]*><\/p>\s*/, '')
}

/** The first `# heading` of a markdown file. */
function title(md) {
  return /^# (.*)$/m.exec(md)?.[1].replace(/`/g, '') ?? 'Un-Banished'
}

const written = []
function page(path, frontmatter, body, source) {
  const fm = Object.entries(frontmatter)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `${k}: ${JSON.stringify(v)}`)
    .join('\n')
  const file = path.endsWith('/') ? `${path}index.md` : `${path}.md`
  const out = join(site, file)
  mkdirSync(dirname(out), { recursive: true })
  const note = `<!-- Generated by site/scripts/prepare.mjs from ${source}. Edit that file instead. -->`
  writeFileSync(out, `---\n${fm}\n---\n\n${note}\n\n${body.trim()}\n`)
  written.push(file)
}

const read = (f) => readFileSync(join(repo, f), 'utf8')
const readme = read('README.md')

// The user guide, split into pages.
const guide = sections(dropLogo(read(GUIDE_FILE)))
for (const p of GUIDE) {
  let body = `# ${p.title}\n\n`
  if (p.intro) body += guide.intro.replace(/^# .*\n/m, '').split('**Setup**')[0].trim() + '\n\n'
  for (const h of p.sections) {
    const s = guide.list.find((s) => s.heading === h)
    if (!s) throw new Error(`${GUIDE_FILE}: section not found: ${h}`)
    body += p.sections.length > 1 || p.intro ? `## ${h}\n\n${s.body}\n\n` : `${s.body}\n\n`
  }
  page(p.path, { title: p.title, description: p.description, outline: [2, 3] }, rewrite(body, GUIDE_FILE), GUIDE_FILE)
}
for (const s of guide.list) {
  if (!guideAnchors.has(slug(s.heading))) throw new Error(`${GUIDE_FILE}: section "${s.heading}" has no page in GUIDE`)
}

// Pages made from whole files.
for (const [file, p] of Object.entries(PAGES)) {
  if (p.site === false) continue
  if (!existsSync(join(repo, file))) {
    console.warn(`prepare: missing ${file}, skipped`)
    continue
  }
  const md = dropLogo(read(file))
  page(p.path, { title: p.title ?? title(md), description: p.description, outline: [2, 3] }, rewrite(md, file), file)
}

// Pages made from sections of the README.
page('/about/status', { title: 'Status', description: 'What works today, and what is still open.' },
  `# Status\n\n${rewrite(section(readme, 'Status'), 'README.md')}`, 'README.md')
page('/about/faq', { title: 'Questions and answers', description: 'Common questions about Un-Banished.' },
  `# Questions and answers\n\n${rewrite(section(readme, 'FAQ'), 'README.md')}`, 'README.md')
page('/about/licence', { title: 'Licence and legal', description: 'The licence of Un-Banished, and its relation to Banished.' },
  `# Licence and legal\n\n## Licence\n\n${rewrite(section(readme, 'Licence'), 'README.md')}\n\n` +
    `## Legal\n\n${rewrite(section(readme, 'Legal'), 'README.md')}`, 'README.md')

// Images and videos: the logo, icons, diagram, screenshots and videos, from docs/assets.
const assets = join(site, 'public', 'assets')
mkdirSync(assets, { recursive: true })
cpSync(join(repo, 'docs', 'assets'), assets, { recursive: true })
cpSync(join(repo, 'docs', 'assets', 'un-banished-icon-small.svg'), join(site, 'public', 'favicon.svg'))

console.log(`prepare: wrote ${written.length} pages and copied docs/assets (${relative(repo, assets)})`)
if (!existsSync(join(site, 'public', 'game', 'index.html'))) {
  console.log('prepare: no game build in public/game; run scripts/build-game.sh to include it')
}
if (!existsSync(join(site, 'public', 'api', 'index.html'))) {
  console.log('prepare: no API documentation in public/api; run scripts/build-api.sh to include it')
}
