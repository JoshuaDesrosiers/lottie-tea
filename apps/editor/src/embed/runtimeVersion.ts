/**
 * The **shared runtime**'s version and where the hosted copy lives.
 *
 * "Export → Website embed (shared runtime)" writes pages that load the
 * engine (`an1mbyt3-embed.js` + `an1mbyt3_wasm_bg.wasm`) from one
 * versioned folder instead of shipping their own copy:
 *
 * ```
 * https://toastycreate.web.app/runtime/<RUNTIME_VERSION>/an1mbyt3-embed.js
 * https://toastycreate.web.app/runtime/<RUNTIME_VERSION>/an1mbyt3_wasm_bg.wasm
 * https://toastycreate.web.app/runtime/toasty-runtime-<RUNTIME_VERSION>.zip
 * ```
 *
 * `scripts/build-runtime.mjs` (run by `npm run build:player`) builds that
 * folder and zip into `public/runtime/`, reading the version from *this
 * file* (the `RUNTIME_VERSION = '…'` line below — keep it a plain string
 * literal). It's the single source of truth: bump it when a new engine
 * can't run games exported for the old one or vice versa, so pages pinned
 * to an older runtime keep loading the folder they were made for (old
 * version folders are never deleted by the build).
 */
export const RUNTIME_VERSION = '0.1.0'

/** Where the hosted runtimes live (the deployed editor site). */
export const HOSTED_RUNTIME_ORIGIN = 'https://toastycreate.web.app'

/** The hosted runtime folder for `version`, with a trailing slash. */
export function hostedRuntimeUrl(version: string = RUNTIME_VERSION): string {
  return `${HOSTED_RUNTIME_ORIGIN}/runtime/${version}/`
}

/** The downloadable zip's file name (`toasty-runtime-<version>.zip`). */
export function runtimeZipName(version: string = RUNTIME_VERSION): string {
  return `toasty-runtime-${version}.zip`
}

/** The site-relative path of the zip (`/runtime/toasty-runtime-<v>.zip`) —
 * served by the editor itself (dev server, web build, desktop bundle). */
export function runtimeZipPath(version: string = RUNTIME_VERSION): string {
  return `/runtime/${runtimeZipName(version)}`
}
