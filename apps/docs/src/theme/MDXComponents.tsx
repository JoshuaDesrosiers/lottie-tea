// Global MDX components: `<Lite>`, `<LiveHTML>`, `<ToastyDemo>`,
// `<ToastyPlayer>` and `<Shot>` work on every page without an import.
import MDXComponents from '@theme-original/MDXComponents'
import { Shot, ToastyDemo, ToastyPlayer } from '@site/src/components/ToastyPlayer'
import { Lite } from '@site/src/components/Lite'
import { LiveHTML } from '@site/src/components/LiveHTML'
import { ScriptLab } from '@site/src/components/ScriptLab'

export default {
  ...MDXComponents,
  Lite,
  LiveHTML,
  ScriptLab,
  ToastyDemo,
  ToastyPlayer,
  Shot,
}
