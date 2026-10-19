/**
 * `<LiteAsterisk>` — the little "*" after anything the Toasty Lite export
 * leaves out (normal mode; Lite mode hides those things instead). Hover
 * says why. See `lite/support.ts`.
 */
import { NOT_IN_LITE } from '../lite/support'

export function LiteAsterisk({ note }: { note?: string }) {
  return (
    <span title={note ? `${note} — ${NOT_IN_LITE}` : NOT_IN_LITE} aria-label="not in Toasty Lite" className="ml-0.5 align-super text-[0.8em] font-semibold text-amber-400/90">
      *
    </span>
  )
}
