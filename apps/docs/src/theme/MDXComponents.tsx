// Global MDX components: `<ToastyDemo>`, `<ToastyPlayer>` and `<Shot>` work
// on every page without an import.
import MDXComponents from '@theme-original/MDXComponents'
import { Shot, ToastyDemo, ToastyPlayer } from '@site/src/components/ToastyPlayer'

export default {
  ...MDXComponents,
  ToastyDemo,
  ToastyPlayer,
  Shot,
}
