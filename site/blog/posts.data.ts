// The blog's posts, newest first, for the blog's front page: release posts (with a version) and
// articles (with a kind, such as "Behind the scenes").
import { createContentLoader } from 'vitepress'

export interface Post {
  url: string
  title: string
  name: string
  /** The release, for a release post. */
  version?: string
  /** What an article is, for a post that is not a release ("Behind the scenes"). */
  kind?: string
  /** The day, YYYY-MM-DD. */
  date: string
  /** The date and time, to order posts of the same day. */
  at: string
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
        version: p.frontmatter.version === undefined ? undefined : String(p.frontmatter.version),
        kind: p.frontmatter.kind,
        // YAML reads an unquoted date as a Date. A date may have a time, to order posts of a day.
        date: new Date(p.frontmatter.date).toISOString().slice(0, 10),
        at: new Date(p.frontmatter.date).toISOString(),
        cover: p.frontmatter.cover,
        summary: p.frontmatter.summary,
      }))
      .sort((a, b) => b.at.localeCompare(a.at) || (b.version ?? '').localeCompare(a.version ?? ''))
  },
})
