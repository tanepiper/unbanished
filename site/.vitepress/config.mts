import { defineConfig } from 'vitepress'

// The site is served from GitHub Pages at https://tanepiper.github.io/unbanished/.
// SITE_BASE=/ builds it for the root of a domain instead.
const base = process.env.SITE_BASE ?? '/unbanished/'
const github = 'https://github.com/tanepiper/unbanished'

export default defineConfig({
  base,
  lang: 'en-GB',
  title: 'Un-Banished',
  description:
    'A new engine for the city-building game Banished. Play on Windows, macOS, Linux or in your browser, with your own copy of the game.',
  cleanUrls: true,
  appearance: 'dark',
  lastUpdated: false,
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
      { text: 'Setup guide', link: '/guide/' },
      { text: 'How it was made', link: '/about/how-it-was-made' },
      {
        text: 'About',
        items: [
          { text: 'About the project', link: '/about/' },
          { text: 'Status', link: '/about/status' },
          { text: 'Questions and answers', link: '/about/faq' },
          { text: 'Licence and legal', link: '/about/licence' },
          { text: 'Contribute', link: '/contribute' },
        ],
      },
    ],
    sidebar: [
      {
        text: 'Play',
        items: [
          { text: 'Play in your browser', link: '/play' },
          { text: 'Setup guide', link: '/guide/' },
        ],
      },
      {
        text: 'About',
        items: [
          { text: 'About the project', link: '/about/' },
          { text: 'How it was made', link: '/about/how-it-was-made' },
          { text: 'Status', link: '/about/status' },
          { text: 'Questions and answers', link: '/about/faq' },
          { text: 'Licence and legal', link: '/about/licence' },
        ],
      },
      {
        text: 'Help',
        items: [
          { text: 'Contribute', link: '/contribute' },
          { text: 'Report a problem', link: `${github}/issues/new/choose` },
        ],
      },
    ],
    socialLinks: [{ icon: 'github', link: github }],
    search: { provider: 'local' },
    outline: { level: [2, 3], label: 'On this page' },
    footer: {
      message:
        'Un-Banished is free software under the MIT or Apache-2.0 licence. It is not affiliated with or endorsed by Shining Rock Software.',
      copyright: 'Banished is © Shining Rock Software, LLC. Screenshots show the original game’s art.',
    },
  },
})
