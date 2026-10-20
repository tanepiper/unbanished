---
name: banished-blog
description: Write or edit a post for the website's blog (site/blog/) - a release post (a few high-level stories, one per feature, closed by a short changelog list) or an article (behind the scenes - how we test, measure or build the game), with pictures and films made by Un-Banished. Use when preparing a release's post, writing an article, or changing an existing post.
---

# Write a blog post

The blog has two kinds of post:

- **Release posts.** Each release has one, named `X-Y-Z-<release-name>.md`, for example
  `0-3-0-neighbours.md`. It tells the player what is new in a few stories, in the style of the
  patch notes of games such as No Man's Sky. The full list of changes stays in `CHANGELOG.md`.
  Its front matter has `version`.
- **Articles**, at any time, named `<subject>.md`, for example `measuring-speed.md`: one subject
  from behind the scenes, such as how we measure the game's speed, with what we found. Its front
  matter has `kind` (for example `Behind the scenes`) and no `version`; the banner and the blog
  list show the kind in place of the version. Give `date` a time
  (`2026-10-20T22:00:00Z`) when another post has the same day, so the newest comes first.

The rules below apply to both. An article has no "Also in" list: it ends with links to the
detailed documents and to the post before it.

## Editorial rules

1. **One story per feature.** Each `##` section is one feature a player would name: multiplayer,
   mods, the browser version, speed in large towns, films of your town. Everything about that
   feature goes inside its story, even when it was several pieces of work. Multiplayer's rooms,
   its lockstep, its desync recovery and its players line are one story, not four.
2. **Few stories.** Three to five, most with a picture or a film. A small feature that has no
   story of its own goes into the changelog list at the end, not into its own section.
3. **High level.** Say what the player can do now and why it matters. Explain how it works only
   as far as a player finds it interesting (for example "the computers send only what the
   players do"). Leave the details to the guide and link to it.
4. **No credits or origins.** Do not name the mods, projects or people that a feature was
   modelled on.
5. **No bug stories.** Do not tell how a bug was found, reported or fixed. Fixes go in the list
   at the end, as one line each, phrased as what works now.
6. **The author's voice**, as on [tane.dev](https://tane.dev): first person ("I", and "we" for
   the project), prose paragraphs rather than bullet lists, spaced hyphens " - " for asides,
   British spelling, concrete facts and examples, plain headings. Open on a real case, not a
   slogan. Avoid AI-ish phrasing: clauses chained with "and the ..." (grep the post for it and
   rewrite each one), neat lists of three, signposts ("Here is how", "Three things stand out")
   and clichés. A playful title is fine.
7. **The ending:** an optional `## What is next`, a short paragraph, then `## Also in X.Y.Z`, a
   short list of the smaller changes and fixes, each a bold phrase and one sentence. The footer
   links to the changelog and to the posts before and after; when adding a post, add the
   **Next:** link to the previous post too.

## Pictures and films

- Every picture and film is made by Un-Banished itself: `tools/screenshots.sh`,
  `tools/videos.sh` (see the `banished-video` skill), or headless captures. Small WebP and
  WebM files only, in `docs/assets/screenshots/` and `docs/assets/videos/`.
- **The banner** (`<ReleaseHero>`) is a 16:9 box that crops its film to fill it. Use a 16:9 film
  (1280 × 720 or 1920 × 1080). A wider film, such as two windows side by side, needs its own
  16:9 cut (see `multiplayer-hero.webm`), or the box shows only its middle.
- **The cover** in the front matter is the banner's poster. It is the post's card on the blog
  list, so it must look clearly different from the other posts' covers.
- Do not use the same picture or film in two posts. If two posts need a similar subject, make a
  new shot.
- Write an `alt` text that says what the picture shows, and a short caption.

## Building blocks

| Block | Use |
|---|---|
| `<ReleaseHero name version date video poster alt />` | The banner at the top |
| `<p class="ub-lead">` | The opening paragraph, under the banner |
| `<Shot src poster alt caption />` | One picture or film |
| `<div class="ub-shots">` around several `<Shot>` | Pictures side by side |
| `<BeforeAfter unit :rows />` | Before-and-after numbers as bars |
| `<FpsGrid caption :columns :rows />` | Frame rates as tiles, a row per town and a column per speed (green at 59 or more, amber from 45, red below) |

The front matter has `title`, `name`, `version` (release posts) or `kind` (articles), `date`,
`cover`, `summary` (the card's text) and `description` (for search engines). `<ReleaseHero>`
takes `version` or `kind` in the same way. `site/blog/posts.data.ts` lists the posts.

## Check before landing

- `npm --prefix site run build` passes with no dead links.
- Look at the post and the blog list at phone width (375 px) and desktop width: the banner,
  the covers and the pictures.
- Read the post against the rules above: one story per feature, no bug stories, a short list at
  the end.
- Commit with the `banished-land` skill.
