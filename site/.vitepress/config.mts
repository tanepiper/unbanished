import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { defineConfig } from 'vitepress'

// The site is served from GitHub Pages at https://tanepiper.github.io/unbanished/.
// SITE_BASE=/ builds it for the root of a domain instead.
const base = process.env.SITE_BASE ?? '/unbanished/'
const github = 'https://github.com/tanepiper/unbanished'

// The engine version, chosen the same way as apps/game/build.rs: the workspace version from
// Cargo.toml, then UNBANISHED_BUILD, the short git commit hash (with -dirty), GITHUB_SHA or
// "unknown".
function engineVersion(): string {
  const cargo = readFileSync(new URL('../../Cargo.toml', import.meta.url), 'utf8')
  const version = /\[workspace\.package\][^[]*?\nversion = "([^"]+)"/.exec(cargo)?.[1] ?? '0.0.0'
  const git = (...args: string[]) =>
    execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim()
  let build = process.env.UNBANISHED_BUILD?.trim()
  if (!build) {
    try {
      build = git('rev-parse', '--short=7', 'HEAD')
      if (git('status', '--porcelain', '--untracked-files=no')) build += '-dirty'
    } catch {
      build = process.env.GITHUB_SHA?.slice(0, 7) || 'unknown'
    }
  }
  return `${version}-${build}`
}

const helpGroup = {
  text: 'Help',
  items: [
    { text: 'Contributing', link: '/develop/contributing' },
    { text: 'Report a problem', link: `${github}/issues/new/choose` },
  ],
}

const guideSidebar = [
  {
    text: 'Play',
    items: [
      { text: 'Play in your browser', link: '/play' },
      { text: 'Setup guide', link: '/guide/' },
      { text: 'Playing the game', link: '/guide/playing' },
      { text: 'Controls', link: '/guide/controls' },
      { text: 'Saves and settings', link: '/guide/saves' },
      { text: 'Mods', link: '/guide/mods' },
      { text: 'The browser version', link: '/guide/browser' },
      { text: 'Command line and version', link: '/guide/command-line' },
      { text: 'Troubleshooting', link: '/guide/troubleshooting' },
    ],
  },
  helpGroup,
]

export default defineConfig({
  base,
  lang: 'en-GB',
  title: 'Un-Banished',
  description:
    'A new engine for the city-building game Banished. Play on Windows, macOS, Linux or in your browser, with your own copy of the game.',
  cleanUrls: true,
  appearance: 'dark',
  lastUpdated: false,
  // The Rust API documentation (/api/) is built by scripts/build-api.sh, outside VitePress.
  ignoreDeadLinks: [/^\/api\//],
  markdown: {
    config(md) {
      // The API pages are plain rustdoc HTML, not VitePress pages: load them normally.
      const linkOpen = md.renderer.rules.link_open
      md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
        const href = tokens[idx].attrGet('href')
        if (href?.startsWith('/api/')) {
          tokens[idx].attrSet('href', base + href.slice(1))
          tokens[idx].attrSet('target', '_self')
        }
        return linkOpen ? linkOpen(tokens, idx, options, env, self) : self.renderToken(tokens, idx, options)
      }
    },
  },
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: `${base}favicon.svg` }],
    ['meta', { name: 'theme-color', content: '#17120d' }],
    ['meta', { property: 'og:title', content: 'Un-Banished' }],
    ['meta', { property: 'og:description', content: 'Banished, rebuilt from scratch in Rust. Play it on any computer, or in your browser.' }],
    ['meta', { property: 'og:image', content: `https://tanepiper.github.io${base}assets/screenshots/town-hall.webp` }],
    ['link', { rel: 'preconnect', href: 'https://fonts.googleapis.com' }],
    ['link', { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' }],
    [
      'link',
      {
        rel: 'stylesheet',
        href: 'https://fonts.googleapis.com/css2?family=EB+Garamond:ital,wght@0,500;0,600;0,700;1,500&display=swap',
      },
    ],
  ],
  themeConfig: {
    logo: '/assets/un-banished-icon-small.svg',
    siteTitle: 'Un-Banished',
    nav: [
      { text: 'Play', link: '/play' },
      { text: 'Guide', link: '/guide/', activeMatch: '^/guide/' },
      { text: 'Develop', link: '/develop/', activeMatch: '^/develop/' },
      { text: 'Topics', link: '/topics/', activeMatch: '^/topics/' },
      { text: 'Reference', link: '/reference/', activeMatch: '^/reference/' },
      {
        text: 'About',
        items: [
          { text: 'About the project', link: '/about/' },
          { text: 'How it was made', link: '/about/how-it-was-made' },
          { text: 'Status', link: '/about/status' },
          { text: 'Changelog', link: '/changelog' },
          { text: 'Questions and answers', link: '/about/faq' },
          { text: 'Licence and legal', link: '/about/licence' },
        ],
      },
    ],
    sidebar: {
      '/guide/': guideSidebar,
      '/play': guideSidebar,
      '/develop/': [
        {
          text: 'Develop',
          items: [
            { text: 'Start here', link: '/develop/' },
            { text: 'Building', link: '/develop/building' },
            { text: 'Testing', link: '/develop/testing' },
            { text: 'Compatibility between versions', link: '/develop/compatibility' },
            { text: 'Performance and profiling', link: '/develop/performance' },
            { text: 'Changing what is drawn', link: '/develop/visual-changes' },
            { text: 'Recording videos', link: '/develop/videos' },
            { text: 'API documentation', link: '/develop/api' },
            { text: 'Contributing', link: '/develop/contributing' },
          ],
        },
        {
          text: 'Development guides',
          items: [
            { text: 'The work loop', link: '/develop/guides/' },
            { text: '1. Trace', link: '/develop/guides/trace' },
            { text: '2. Emulate', link: '/develop/guides/emulate' },
            { text: '3. Decode a format', link: '/develop/guides/format' },
            { text: '4. Port', link: '/develop/guides/port' },
            { text: '5. Port a shader', link: '/develop/guides/shader' },
            { text: '6. Verify', link: '/develop/guides/verify' },
            { text: '7. Document and land', link: '/develop/guides/land' },
          ],
        },
        helpGroup,
      ],
      '/topics/': [
        {
          text: 'How it works',
          items: [
            { text: 'Overview', link: '/topics/' },
            { text: 'Game data files', link: '/topics/game-data' },
            { text: 'Exact maths and random numbers', link: '/topics/exact-maths' },
            { text: 'Terrain generation', link: '/topics/terrain' },
            { text: 'The game loop', link: '/topics/game-loop' },
            { text: 'Citizens', link: '/topics/citizens' },
            { text: 'Save files', link: '/topics/saves' },
            { text: 'The interface', link: '/topics/interface' },
            { text: 'Audio', link: '/topics/audio' },
            { text: 'The emulator harness', link: '/topics/emulator' },
          ],
        },
        helpGroup,
      ],
      '/reference/': [
        {
          text: 'Apps',
          items: [
            { text: 'All apps and crates', link: '/reference/' },
            { text: 'banished (the game)', link: '/reference/game' },
            { text: 'banished-viewer', link: '/reference/viewer' },
            { text: 'banished-cli', link: '/reference/cli' },
          ],
        },
        {
          text: 'Libraries',
          items: [
            { text: 'banished-core', link: '/reference/core' },
            { text: 'banished-terrain', link: '/reference/terrain' },
            { text: 'banished-path', link: '/reference/path' },
            { text: 'banished-game', link: '/reference/game-rules' },
            { text: 'banished-formats', link: '/reference/formats' },
            { text: 'banished-ui', link: '/reference/ui' },
            { text: 'banished-gfx', link: '/reference/gfx' },
            { text: 'banished-audio', link: '/reference/audio' },
            { text: 'banished-wma', link: '/reference/wma' },
            { text: 'API documentation', link: '/develop/api' },
          ],
        },
        helpGroup,
      ],
      '/': [
        {
          text: 'About',
          items: [
            { text: 'About the project', link: '/about/' },
            { text: 'How it was made', link: '/about/how-it-was-made' },
            { text: 'Status', link: '/about/status' },
            { text: 'Changelog', link: '/changelog' },
            { text: 'Questions and answers', link: '/about/faq' },
            { text: 'Licence and legal', link: '/about/licence' },
          ],
        },
        helpGroup,
      ],
    },
    socialLinks: [{ icon: 'github', link: github }],
    search: { provider: 'local' },
    outline: { level: [2, 3], label: 'On this page' },
    footer: {
      message:
        `Un-Banished ${engineVersion()} is free software under the MIT or Apache-2.0 licence. It is not affiliated with or endorsed by Shining Rock Software.`,
      copyright: 'Banished is © Shining Rock Software, LLC. Screenshots show the original game’s art.',
    },
  },
})
