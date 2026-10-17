/**
 * The editor's own `alert` / `confirm` / `prompt` — themed modal dialogs in
 * place of the browser's native ones (which show "toastycreate.web.app
 * says", use OS chrome and block the page).
 *
 * Imperative and promise-based so call sites stay one-liners:
 *
 * ```ts
 * showAlert('Export failed: …')
 * if (!(await showConfirm('Delete this?', { danger: true, okLabel: 'Delete' }))) return
 * const name = await showPrompt('Project name:', 'My Project')   // null = cancelled
 * ```
 *
 * Self-mounting: the first call creates one host (`<div>` + React root) on
 * `document.body`, so nothing has to be added to `App`. Dialogs queue — one
 * shows at a time. Enter = OK, Esc / backdrop = Cancel; focus starts on the
 * field (prompt) or the OK button.
 */
import { useEffect, useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'

interface DialogRequest {
  kind: 'alert' | 'confirm' | 'prompt'
  message: string
  title?: string
  okLabel?: string
  cancelLabel?: string
  /** Red OK button (deleting, discarding). */
  danger?: boolean
  defaultValue?: string
  resolve: (value: boolean | string | null) => void
}

let push: ((req: DialogRequest) => void) | null = null
const pending: DialogRequest[] = []

function ensureHost() {
  if (push || typeof document === 'undefined') return
  const el = document.createElement('div')
  el.setAttribute('data-popover', '') // outside-click handlers treat it as inside
  document.body.appendChild(el)
  createRoot(el).render(<DialogHost />)
}

function request(req: Omit<DialogRequest, 'resolve'>): Promise<boolean | string | null> {
  return new Promise((resolve) => {
    const full = { ...req, resolve }
    if (push) push(full)
    else {
      pending.push(full)
      ensureHost()
    }
  })
}

export interface DialogOptions {
  title?: string
  okLabel?: string
  cancelLabel?: string
  danger?: boolean
}

/** A message with an OK button. Resolves when dismissed. */
export function showAlert(message: string, options: DialogOptions = {}): Promise<void> {
  return request({ kind: 'alert', message, ...options }).then(() => undefined)
}

/** OK / Cancel. Resolves `true` for OK. */
export function showConfirm(message: string, options: DialogOptions = {}): Promise<boolean> {
  return request({ kind: 'confirm', message, ...options }).then((v) => v === true)
}

/** A text field. Resolves the text, or `null` if cancelled. */
export function showPrompt(message: string, defaultValue = '', options: DialogOptions = {}): Promise<string | null> {
  return request({ kind: 'prompt', message, defaultValue, ...options }).then((v) => (typeof v === 'string' ? v : null))
}

function DialogHost() {
  const [queue, setQueue] = useState<DialogRequest[]>(() => pending.splice(0))
  useEffect(() => {
    push = (req) => setQueue((q) => [...q, req])
    return () => {
      push = null
    }
  }, [])
  const current = queue[0]
  if (!current) return null
  const close = (value: boolean | string | null) => {
    current.resolve(value)
    setQueue((q) => q.slice(1))
  }
  return <Dialog key={queue.length + current.message} req={current} onClose={close} />
}

function Dialog({ req, onClose }: { req: DialogRequest; onClose: (value: boolean | string | null) => void }) {
  const [text, setText] = useState(req.defaultValue ?? '')
  const ok = useRef<HTMLButtonElement>(null)
  const field = useRef<HTMLInputElement>(null)
  const cancelValue = req.kind === 'prompt' ? null : false
  const submit = () => onClose(req.kind === 'prompt' ? text : true)
  useEffect(() => {
    if (req.kind === 'prompt') {
      field.current?.focus()
      field.current?.select()
    } else ok.current?.focus()
  }, [req.kind])
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        onClose(req.kind === 'alert' ? true : cancelValue)
      }
    }
    window.addEventListener('keydown', key, true)
    return () => window.removeEventListener('keydown', key, true)
  })
  const title = req.title ?? (req.kind === 'confirm' ? 'Are you sure?' : req.kind === 'prompt' ? 'Enter a value' : 'Toasty')
  return (
    <div
      className="fixed inset-0 grid place-items-center bg-black/45 p-4 backdrop-blur-[2px]"
      style={{ zIndex: 20000, animation: 'an1-dialog-fade .12s ease-out' }}
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) onClose(req.kind === 'alert' ? true : cancelValue)
      }}
    >
      <style>{'@keyframes an1-dialog-fade{from{opacity:0}}@keyframes an1-dialog-pop{from{opacity:0;transform:translateY(6px) scale(.98)}}'}</style>
      <div
        role={req.kind === 'alert' ? 'alertdialog' : 'dialog'}
        aria-modal="true"
        aria-label={title}
        className="w-full max-w-sm rounded-xl border border-panel-border bg-solid-panel p-4 text-ink shadow-2xl"
        style={{ animation: 'an1-dialog-pop .16s ease-out' }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault()
            submit()
          }
        }}
      >
        <div className="mb-1.5 text-sm font-semibold">{title}</div>
        <p className="whitespace-pre-wrap break-words text-[13px] leading-relaxed text-ink-dim">{req.message}</p>
        {req.kind === 'prompt' && (
          <input ref={field} value={text} onChange={(e) => setText(e.target.value)} className="mt-3 w-full rounded-md border border-panel-border bg-overlay/5 px-2.5 py-1.5 text-[13px] text-ink outline-none focus:border-accent" />
        )}
        <div className="mt-4 flex justify-end gap-2">
          {req.kind !== 'alert' && (
            <button onClick={() => onClose(cancelValue)} className="rounded-md px-3 py-1.5 text-[13px] text-ink-dim hover:bg-overlay/8 hover:text-ink">
              {req.cancelLabel ?? 'Cancel'}
            </button>
          )}
          <button
            ref={ok}
            onClick={submit}
            className={`rounded-md px-3.5 py-1.5 text-[13px] font-medium text-white outline-none focus-visible:ring-2 focus-visible:ring-accent/60 ${req.danger ? 'bg-red-500 hover:bg-red-500/85' : 'bg-accent hover:bg-accent/85'}`}
          >
            {req.okLabel ?? 'OK'}
          </button>
        </div>
      </div>
    </div>
  )
}
