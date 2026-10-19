// The blog's posts, newest first, for the blog's front page.
import { createContentLoader } from 'vitepress'

export interface Post {
  url: string
  title: string
  name: string
  version: string
  date: string
  cover: string
  summary: string
}

declare const data: Post[]
export { data }

export default createContentLoader('blog/*.md', {
  transform(raw): Post[] {
    return raw
      .filter((p) => !p.url.endsWith('/blog/'))
      .map((p) => ({
        url: p.url,
        title: p.frontmatter.title,
        name: p.frontmatter.name,
        version: String(p.frontmatter.version),
        // YAML reads an unquoted date as a Date.
        date: new Date(p.frontmatter.date).toISOString().slice(0, 10),
        cover: p.frontmatter.cover,
        summary: p.frontmatter.summary,
      }))
      .sort((a, b) => (a.date === b.date ? b.version.localeCompare(a.version) : b.date.localeCompare(a.date)))
  },
})
