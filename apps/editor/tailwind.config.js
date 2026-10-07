/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      // Restrained dark palette + single accent color (AN1MBYT3_BUILD_PLAN.md
      // §15) — deliberately a short list. A per-node-type color palette (one
      // color per tag, cartex-style) was tried and reverted after seeing it
      // rendered — it read as too loud. Adding more named colors here is
      // exactly the kind of "rainbow of semantic colors competing for
      // attention" §15 says to avoid; reach for `canvas`/`panel`/`border`
      // variants of these tokens (or plain `white/N` opacity, as the Layers
      // pill tags now use) before introducing a new color name.
      // Each named here as a CSS custom property reference, not a static
      // hex — `index.css` defines the actual values twice (`:root` for
      // dark, `:root[data-theme="light"]` for light; see its own doc
      // comment), which is what makes `TitleBar`'s theme toggle able to
      // flip every one of these at once, at runtime, with no React
      // re-render needed — Tailwind itself just emits `bg-canvas { background-color:
      // var(--color-canvas) }` once at build time regardless of which
      // theme is active.
      colors: {
        canvas: 'var(--color-canvas)',
        panel: 'var(--color-panel)',
        'panel-border': 'var(--color-panel-border)',
        ink: 'var(--color-ink)',
        'ink-dim': 'var(--color-ink-dim)',
        // A function (like `overlay` below) so opacity modifiers work:
        // `bg-accent/20` -> `rgb(var(--accent-rgb) / 0.2)`.
        accent: ({ opacityValue }) => (opacityValue !== undefined ? `rgb(var(--accent-rgb) / ${opacityValue})` : 'rgb(var(--accent-rgb))'),
        // An always-*opaque* panel surface, for things that need to read as
        // their own distinct card/surface rather than a translucent overlay
        // on whatever's behind them — `StateMachineGraph`'s own state boxes
        // (`bg-overlay/8` read as a barely-there wash, especially in light
        // mode, rather than a real surface popping against the graph
        // canvas behind it) are the first consumer. Same token the forked-in
        // `scratch-paint` CSS already uses (`index.css`'s own doc comment).
        'solid-panel': 'var(--solid-panel)',
        // The "subtle neutral chrome" color — every hover state/divider/
        // border across this app used to be a literal `white/N` opacity
        // utility (reads fine laid over the dark canvas these were all
        // designed against), which is exactly why the light theme first
        // came out "ugly": those same overlays go nearly invisible (a
        // near-white tint over an already-light panel reads as no border/
        // no hover at all) instead of flipping polarity. This color
        // function is Tailwind's own documented pattern for an opacity-
        // modifiable color backed by a CSS var (`--overlay-rgb`, an
        // `R G B` triplet, not a hex) — `index.css` sets it to white in
        // dark mode (`255 255 255`, identical to the old literal) and
        // black in light mode (`0 0 0`), so `bg-overlay/8`/`border-overlay/10`/
        // etc. read exactly the same as the old `white/N` classes did in
        // dark mode and automatically become dark-on-light in light mode —
        // the actual Figma/Fresco-style "subtle neutral chrome" convention
        // this was missing. A literal `bg-white`/`text-white` *without* a
        // slash (no opacity) is a different, deliberate thing — solid white
        // text/icons on a colored button, meant to stay white regardless of
        // theme — and is untouched.
        overlay: ({ opacityValue }) => (opacityValue !== undefined ? `rgb(var(--overlay-rgb) / ${opacityValue})` : 'rgb(var(--overlay-rgb))'),
      },
      backdropBlur: {
        panel: '20px',
      },
    },
  },
  plugins: [],
}
