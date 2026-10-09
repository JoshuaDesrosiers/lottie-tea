/**
 * **Google Fonts** and **Material Symbols** (Google's icon font) for the
 * editor, exported games and the Paint tool.
 *
 * Two different consumers need two different files:
 * - Anything the *browser* draws (HTML UI, Paint's text tool, previews in
 *   pickers) uses Google's own stylesheet API (`fonts.googleapis.com/css2`),
 *   linked once into `document.head` (`loadFontCss`). Fonts declared that
 *   way are visible inside HTML UI shadow roots too — `@font-face` is
 *   document-wide, only class rules stop at a shadow boundary (which is why
 *   `ICON_CLASS_CSS` exists).
 * - Text *nodes* are shaped and tessellated by the engine
 *   (`an1mbyt3-text`, rustybuzz + ttf-parser), which reads TTF / OTF but
 *   not the WOFF2 Google's API serves browsers. `fetchFontFile` gets the
 *   same font as a TTF from Fontsource's CDN (Fontsource republishes every
 *   Google Font, OFL-licensed), and the bytes go into the document as a
 *   `FontAsset` (`WasmEditor::add_font`) — so a saved project / exported
 *   game carries its fonts and never needs the network to draw text.
 *
 * Icons are a font too: Material Symbols maps an icon's *name* to its glyph
 * with a ligature, so a Text node whose font is Material Symbols and whose
 * text is `home` draws the house icon — in the engine (rustybuzz applies
 * `liga`) and in HTML (`<span class="material-symbols-rounded">home</span>`).
 *
 * Everything here works offline with a smaller built-in list
 * (`POPULAR_FONTS`, `COMMON_ICONS`); the full catalogues (~2000 families,
 * ~4000 icons) are fetched on first use and cached in `localStorage`.
 */

export type FontCategory = 'sans-serif' | 'serif' | 'display' | 'handwriting' | 'monospace' | 'icons'

export interface GoogleFont {
  /** The family name, exactly as Google spells it ("Press Start 2P"). */
  family: string
  /** Fontsource's id for it ("press-start-2p"). */
  id: string
  category: FontCategory
  /** Weights the family ships (400 is always tried first). */
  weights: number[]
  /** The subset with the Latin glyphs, usually `latin`. */
  subset: string
}

const font = (family: string, category: FontCategory, weights = [400, 700]): GoogleFont => ({
  family,
  id: family.toLowerCase().replace(/\s+/g, '-'),
  category,
  weights,
  subset: 'latin',
})

/** A hand-picked list that is always available (also the list's top). */
export const POPULAR_FONTS: GoogleFont[] = [
  font('Inter', 'sans-serif'),
  font('Roboto', 'sans-serif'),
  font('Open Sans', 'sans-serif'),
  font('Montserrat', 'sans-serif'),
  font('Poppins', 'sans-serif'),
  font('Lato', 'sans-serif'),
  font('Nunito', 'sans-serif'),
  font('Rubik', 'sans-serif'),
  font('Work Sans', 'sans-serif'),
  font('Outfit', 'sans-serif'),
  font('Fredoka', 'sans-serif'),
  font('Russo One', 'sans-serif', [400]),
  font('Oswald', 'sans-serif'),
  font('Bebas Neue', 'display', [400]),
  font('Anton', 'sans-serif', [400]),
  font('Bangers', 'display', [400]),
  font('Luckiest Guy', 'display', [400]),
  font('Lilita One', 'display', [400]),
  font('Bungee', 'display', [400]),
  font('Black Ops One', 'display', [400]),
  font('Orbitron', 'sans-serif'),
  font('Audiowide', 'display', [400]),
  font('Press Start 2P', 'display', [400]),
  font('VT323', 'monospace', [400]),
  font('Silkscreen', 'display'),
  font('Pixelify Sans', 'display'),
  font('Creepster', 'display', [400]),
  font('Cinzel', 'serif'),
  font('Playfair Display', 'serif'),
  font('Merriweather', 'serif'),
  font('Lora', 'serif'),
  font('EB Garamond', 'serif'),
  font('Pacifico', 'handwriting', [400]),
  font('Lobster', 'display', [400]),
  font('Caveat', 'handwriting'),
  font('Permanent Marker', 'handwriting', [400]),
  font('Indie Flower', 'handwriting', [400]),
  font('Dancing Script', 'handwriting'),
  font('Fira Code', 'monospace'),
  font('JetBrains Mono', 'monospace'),
  font('Space Mono', 'monospace'),
]

/** The three Material Symbols styles. */
export const ICON_STYLES = ['Rounded', 'Outlined', 'Sharp'] as const
export type IconStyle = (typeof ICON_STYLES)[number]

/** `Material Symbols Rounded` etc. */
export function iconFamily(style: IconStyle): string {
  return `Material Symbols ${style}`
}

/** Whether `family` is one of the icon fonts (its text is icon names). */
export function isIconFamily(family: string | null | undefined): boolean {
  return !!family && /^Material Symbols /.test(family)
}

/** Icons that are always in the picker (and its order before searching). */
export const COMMON_ICONS = [
  'home', 'settings', 'search', 'menu', 'close', 'check', 'add', 'remove', 'favorite', 'star',
  'play_arrow', 'pause', 'stop', 'replay', 'skip_next', 'skip_previous', 'volume_up', 'volume_off', 'music_note', 'mic',
  'arrow_back', 'arrow_forward', 'arrow_upward', 'arrow_downward', 'chevron_left', 'chevron_right', 'expand_more', 'expand_less', 'refresh', 'sync',
  'person', 'group', 'account_circle', 'lock', 'lock_open', 'key', 'shield', 'visibility', 'visibility_off', 'info',
  'warning', 'error', 'help', 'notifications', 'mail', 'chat', 'share', 'download', 'upload', 'delete',
  'edit', 'save', 'folder', 'image', 'photo_camera', 'videocam', 'map', 'location_on', 'flag', 'bookmark',
  'shopping_cart', 'storefront', 'paid', 'diamond', 'trophy', 'military_tech', 'emoji_events', 'bolt', 'local_fire_department', 'water_drop',
  'sports_esports', 'stadia_controller', 'joystick', 'swords', 'skull', 'pets', 'rocket_launch', 'castle', 'potted_plant', 'eco',
  'light_mode', 'dark_mode', 'cloud', 'sunny', 'ac_unit', 'thunderstorm', 'timer', 'hourglass_empty', 'schedule', 'calendar_today',
  'grid_view', 'list', 'dashboard', 'leaderboard', 'bar_chart', 'pie_chart', 'trending_up', 'fullscreen', 'fullscreen_exit', 'open_in_new',
]

// ---- stylesheets (browser-drawn text) ----

const linked = new Set<string>()

function addStylesheet(href: string): void {
  if (linked.has(href) || typeof document === 'undefined') return
  linked.add(href)
  const link = document.createElement('link')
  link.rel = 'stylesheet'
  link.href = href
  link.dataset.an1Font = '1'
  document.head.appendChild(link)
}

/** Google's css2 URL for `family` (regular + bold, or only `text`'s glyphs). */
export function googleCssUrl(family: string, opts: { text?: string; weights?: number[] } = {}): string {
  const name = encodeURIComponent(family).replace(/%20/g, '+')
  const weights = opts.weights ?? (isIconFamily(family) ? null : [400, 700])
  const axis = isIconFamily(family) ? ':opsz,wght,FILL,GRAD@20..48,100..700,0..1,-50..200' : weights && weights.length > 1 ? `:wght@${weights.join(';')}` : ''
  const text = opts.text ? `&text=${encodeURIComponent(opts.text)}` : ''
  return `https://fonts.googleapis.com/css2?family=${name}${axis}&display=swap${text}`
}

/** Makes `family` usable in CSS on this page (HTML UI, Paint text). */
export function loadFontCss(family: string, weights?: number[]): void {
  const known = findFont(family)
  // A family that ships only one weight 400s on a `wght@400;700` request.
  addStylesheet(googleCssUrl(family, { weights: weights ?? (known && known.weights.length < 2 ? [] : undefined) }))
}

/** Loads just the glyphs of `sample` — cheap previews in a long list. */
export function loadFontPreview(family: string, sample = family): void {
  if (isIconFamily(family)) return loadFontCss(family)
  addStylesheet(googleCssUrl(family, { text: sample, weights: [] }))
}

/** Resolves once `family` can draw (canvas text needs this before drawing). */
export async function ensureFontReady(family: string, weights?: number[]): Promise<void> {
  loadFontCss(family, weights)
  try {
    await document.fonts.load(`40px "${family}"`, isIconFamily(family) ? 'home' : 'Ag')
  } catch {
    // The page still falls back to a system font.
  }
}

/** The CSS a shadow root needs for `<span class="material-symbols-rounded">`
 * (Google's stylesheet defines these classes in the document only). */
export const ICON_CLASS_CSS = ICON_STYLES.map(
  (s) => `.material-symbols-${s.toLowerCase()}{font-family:'${iconFamily(s)}';font-weight:normal;font-style:normal;font-size:24px;line-height:1;letter-spacing:normal;text-transform:none;display:inline-block;white-space:nowrap;word-wrap:normal;direction:ltr;-webkit-font-feature-settings:'liga';font-feature-settings:'liga';-webkit-font-smoothing:antialiased;vertical-align:middle}`,
).join('\n')

/** Fonts every OS has — never requested from Google. */
const SYSTEM_FONTS = new Set(['arial', 'helvetica', 'helvetica neue', 'times', 'times new roman', 'georgia', 'verdana', 'tahoma', 'trebuchet ms', 'courier', 'courier new', 'segoe ui', 'impact', 'comic sans ms', 'consolas', 'menlo', 'monaco', '-apple-system', 'blinkmacsystemfont'])

const GENERIC = new Set(['serif', 'sans-serif', 'monospace', 'cursive', 'fantasy', 'system-ui', 'inherit', 'initial', 'unset', 'ui-sans-serif', 'ui-serif', 'ui-monospace', 'emoji', 'math'])

/**
 * Links every Google Font that `css` (or HTML with inline styles) names in
 * a `font-family`, and the icon fonts if it uses `material-symbols-*`
 * classes — so HTML UI authors just write `font-family: 'Bungee'` and
 * `<span class="material-symbols-rounded">home</span>`.
 */
export function loadFontsUsedIn(...sources: string[]): void {
  for (const text of sources) {
    if (!text) continue
    for (const m of text.matchAll(/font-family\s*:\s*([^;}"<]+|"[^"]*")/gi)) {
      for (const raw of m[1].split(',')) {
        const name = raw.trim().replace(/^['"]|['"]$/g, '').replace(/&quot;|&#39;/g, '').trim()
        if (!name || GENERIC.has(name.toLowerCase()) || SYSTEM_FONTS.has(name.toLowerCase()) || name.startsWith('var(')) continue
        // A family the catalogue doesn't know (it isn't loaded in exported
        // games) is still requested — regular weight only, since asking for
        // a weight a family lacks fails the whole stylesheet.
        const known = findFont(name)
        loadFontCss(known?.family ?? name, known ? undefined : [])
      }
    }
    for (const m of text.matchAll(/material-symbols-(rounded|outlined|sharp)/gi)) {
      const style = (m[1][0].toUpperCase() + m[1].slice(1).toLowerCase()) as IconStyle
      loadFontCss(iconFamily(style))
    }
  }
}

// ---- catalogues ----

const CATALOG_KEY = 'an1mbyt3.googleFonts.v1'
let catalog: GoogleFont[] | null = null
let catalogPromise: Promise<GoogleFont[]> | null = null

function readCached<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : null
  } catch {
    return null
  }
}

function writeCached(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // Storage full / blocked: the catalogue is just fetched again next time.
  }
}

/** Every Google Font (popular ones first). Falls back to `POPULAR_FONTS`. */
export function fontCatalog(): Promise<GoogleFont[]> {
  if (catalog) return Promise.resolve(catalog)
  if (!catalogPromise) {
    catalogPromise = (async () => {
      let list = readCached<GoogleFont[]>(CATALOG_KEY)
      if (!list) {
        try {
          const res = await fetch('https://api.fontsource.org/v1/fonts?type=google')
          const rows = (await res.json()) as { id: string; family: string; category: string; weights: number[]; subsets: string[]; defSubset: string }[]
          list = rows
            .filter((r) => r.category !== 'icons' && r.subsets.includes('latin'))
            .map((r) => ({ family: r.family, id: r.id, category: r.category as FontCategory, weights: r.weights, subset: 'latin' }))
          writeCached(CATALOG_KEY, list)
        } catch {
          return POPULAR_FONTS
        }
      }
      const popular = new Set(POPULAR_FONTS.map((f) => f.family))
      catalog = [...POPULAR_FONTS.map((p) => list!.find((f) => f.family === p.family) ?? p), ...list.filter((f) => !popular.has(f.family))]
      return catalog
    })()
  }
  return catalogPromise
}

/** A family by name (from the loaded catalogue, or the popular list). */
export function findFont(family: string): GoogleFont | undefined {
  const lower = family.toLowerCase()
  return (catalog ?? POPULAR_FONTS).find((f) => f.family.toLowerCase() === lower)
}

const ICONS_KEY = 'an1mbyt3.materialSymbols.v1'
let iconNames: string[] | null = null

/** Every Material Symbols icon name (common ones first). */
export async function iconCatalog(): Promise<string[]> {
  if (iconNames) return iconNames
  let names = readCached<string[]>(ICONS_KEY)
  if (!names) {
    try {
      const res = await fetch('https://raw.githubusercontent.com/google/material-design-icons/master/variablefont/MaterialSymbolsRounded%5BFILL%2CGRAD%2Copsz%2Cwght%5D.codepoints')
      names = (await res.text())
        .split('\n')
        .map((l) => l.split(' ')[0])
        .filter(Boolean)
      writeCached(ICONS_KEY, names)
    } catch {
      return COMMON_ICONS
    }
  }
  const common = new Set(COMMON_ICONS)
  iconNames = [...COMMON_ICONS, ...[...new Set(names)].filter((n) => !common.has(n))]
  return iconNames
}

// ---- font files (engine-drawn text) ----

const fileCache = new Map<string, Promise<Uint8Array>>()

/**
 * A family's TTF (regular weight, Latin), for a `FontAsset`. Icon fonts
 * come whole (every icon, with their ligatures). Rejects offline / for an
 * unknown family.
 */
export function fetchFontFile(family: string, weight = 400): Promise<Uint8Array> {
  const key = `${family}@${weight}`
  let p = fileCache.get(key)
  if (!p) {
    const known = findFont(family)
    const id = known?.id ?? family.toLowerCase().replace(/\s+/g, '-')
    const w = known && !known.weights.includes(weight) ? known.weights[0] : weight
    p = fetch(`https://cdn.jsdelivr.net/fontsource/fonts/${id}@latest/${known?.subset ?? 'latin'}-${w}-normal.ttf`).then(async (res) => {
      if (!res.ok) throw new Error(`${family} isn't available as a font file (HTTP ${res.status})`)
      return new Uint8Array(await res.arrayBuffer())
    })
    p.catch(() => fileCache.delete(key))
    fileCache.set(key, p)
  }
  return p
}
