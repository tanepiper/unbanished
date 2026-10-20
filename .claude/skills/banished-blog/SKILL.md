---
name: banished-blog
description: Write or edit a release post for the website's blog (site/blog/), in the project's editorial style - a few high-level stories, one per feature, with pictures and films made by Un-Banished, closed by a short changelog list. Use when preparing a release's blog post, or when changing an existing post.
---

# Write a release post

Each release has one post in `site/blog/`, named `X-Y-Z-<release-name>.md`, for example
`0-3-0-neighbours.md`. It tells the player what is new in a few stories, in the style of the
patch notes of games such as No Man's Sky. The full list of changes stays in `CHANGELOG.md`.

## Editorial rules

1. **One story per feature.** Each `##` section is one feature a player would name: multiplayer,
   mods, the browser version, speed in large towns, films of your town. Everything about that
   feature goes inside its story, even when it was several pieces of work. Multiplayer's rooms,
   its lockstep, its desync recovery and its players line are one story, not four.
2. **Few stories.** Three to five, each with a picture or a film. A small feature that has no
   story of its own goes into the changelog list at the end, not into its own section.
3. **High level.** Say what the player can do now and why it matters. Explain how it works only
   as far as a player finds it interesting (for example "the computers send only what the
   players do"). Leave the details to the guide and link to it.
4. **No credits or origins.** Do not name the mods, projects or people that a feature was
   modelled on.
5. **No bug stories.** Do not tell how a bug was found, reported or fixed. Fixes go in the list
   at the end, as one line each, phrased as what works now.
6. **Plain English** that translates well: short sentences, common words, no idioms or puns in
   the body text. A playful title is fine.
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

The front matter has `title`, `name`, `version`, `date`, `cover`, `summary` (the card's text)
and `description` (for search engines). `site/blog/posts.data.ts` lists the posts.

## Check before landing

- `npm --prefix site run build` passes with no dead links.
- Look at the post and the blog list at phone width (375 px) and desktop width: the banner,
  the covers and the pictures.
- Read the post against the rules above: one story per feature, no bug stories, a short list at
  the end.
- Commit with the `banished-land` skill.
