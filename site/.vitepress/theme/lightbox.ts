// Every picture and video on the site opens in a lightbox (PhotoSwipe, with its video plugin):
// a click (or Enter) on it shows it large over the page, with the page's other pictures and
// videos before and after it, to move through with the arrow keys or a swipe.
// PhotoSwipe loads on the first click, so it costs nothing on a page that is only read.
import 'photoswipe/style.css'

/** The media that open: in a page's text, the post banners and the home page's film. */
const MEDIA = '.vp-doc img, .vp-doc video, .ub-release-hero video, .ub-hero-video'

/** Whether an element opens in the lightbox: not a link to somewhere else, not an icon. */
function opens(el: HTMLElement): boolean {
  if (el.closest('[data-no-lightbox]')) return false
  const link = el.closest('a')
  if (link && el instanceof HTMLImageElement && link.href !== el.currentSrc && link.href !== el.src)
    return false
  if (el instanceof HTMLImageElement) {
    // Badges and small icons stay as they are.
    const w = el.naturalWidth || el.width
    if (w && w < 160) return false
    if (/\.svg($|\?)/.test(el.src)) return false
  }
  return true
}

function media(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>(MEDIA)].filter(opens)
}

/** A lightbox item for a picture or a video, at its own size (a small picture is not blown up). */
function item(el: HTMLElement) {
  if (el instanceof HTMLVideoElement) {
    return {
      type: 'video',
      videoSrc: el.currentSrc || el.src,
      msrc: el.poster || undefined,
      width: el.videoWidth || 1280,
      height: el.videoHeight || 720,
      alt: el.getAttribute('aria-label') ?? '',
    }
  }
  const img = el as HTMLImageElement
  return {
    src: img.currentSrc || img.src,
    msrc: img.currentSrc || img.src,
    width: img.naturalWidth || 1280,
    height: img.naturalHeight || 720,
    alt: img.alt,
  }
}

/** Waits (briefly) for a video's size, which a video that has not started loading lacks. */
function sized(v: HTMLVideoElement): Promise<void> {
  if (v.videoWidth) return Promise.resolve()
  return new Promise((resolve) => {
    const done = () => resolve()
    v.addEventListener('loadedmetadata', done, { once: true })
    setTimeout(done, 1500)
    if (v.preload === 'none') v.preload = 'metadata'
    v.load()
  })
}

async function open(el: HTMLElement) {
  const all = media()
  const index = Math.max(0, all.indexOf(el))
  await Promise.all(all.filter((m): m is HTMLVideoElement => m instanceof HTMLVideoElement).map(sized))
  const [{ default: PhotoSwipeLightbox }, { default: PhotoSwipeVideoPlugin }] = await Promise.all([
    import('photoswipe/lightbox'),
    import('photoswipe-video-plugin'),
  ])
  const lightbox = new PhotoSwipeLightbox({
    dataSource: all.map(item),
    pswpModule: () => import('photoswipe'),
    bgOpacity: 0.92,
    showHideAnimationType: 'fade',
  })
  // The films have no sound: they play muted (so the browser lets them start) and loop.
  new PhotoSwipeVideoPlugin(lightbox, {
    videoAttributes: { controls: '', playsinline: '', preload: 'auto', muted: '', loop: '' },
  })
  lightbox.init()
  lightbox.loadAndOpen(index)
}

/** Marks the page's media as buttons (cursor, focus, a label for screen readers). */
function mark() {
  for (const el of media()) {
    if (el.dataset.lightbox) continue
    el.dataset.lightbox = '1'
    el.tabIndex = 0
    el.setAttribute('role', 'button')
    const what = el instanceof HTMLVideoElement ? 'video' : 'picture'
    const name = el.getAttribute('alt') || el.getAttribute('aria-label') || ''
    el.setAttribute('aria-label', name ? `${name} (opens the ${what} large)` : `Open the ${what} large`)
  }
}

let started = false

/** Called once in the browser, and again after every page change. */
export function lightbox() {
  if (typeof window === 'undefined') return
  // Pictures that load later (lazily) are marked when they arrive.
  setTimeout(mark, 0)
  if (started) return
  started = true
  document.addEventListener('load', (e) => {
    if (e.target instanceof HTMLImageElement) mark()
  }, true)
  document.addEventListener('click', (e) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>(MEDIA)
    if (!el || !opens(el)) return
    e.preventDefault()
    e.stopPropagation()
    open(el)
  }, true)
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter' && e.key !== ' ') return
    const el = e.target as HTMLElement
    if (!el?.matches?.(MEDIA) || !opens(el)) return
    e.preventDefault()
    open(el)
  })
}
