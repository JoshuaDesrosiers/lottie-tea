export default {
  plugins: {
    // `postcss-import`/`postcss-simple-vars`: the vendored scratch-paint
    // CSS (`src/vendor/scratch-paint` — forked in directly, see
    // `PaintAssetEditor.tsx`'s own doc comment) was authored for its own
    // original webpack config, which ran these same two plugins — plain
    // CSS `@import` of another stylesheet (Vite/PostCSS don't inline that
    // by default, only Sass-style preprocessors do) and `$variable`
    // syntax (not real CSS custom properties) for its shared
    // colors/units tokens. Both are no-ops against every *other* stylesheet
    // in this app (Tailwind's own directives, this app's own `index.css`)
    // since neither uses `@import` or `$`-prefixed variables.
    'postcss-import': {},
    'postcss-simple-vars': {},
    tailwindcss: {},
    autoprefixer: {},
  },
}
