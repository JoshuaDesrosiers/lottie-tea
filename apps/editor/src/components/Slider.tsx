import { useRef, type PointerEvent as ReactPointerEvent } from 'react'

/**
 * The app's own slider (custom-drawn track + thumb, pointer-captured drag)
 * instead of a native `<input type="range">`, which reads as obvious
 * browser chrome inside the custom dark UI — see InspectorPanel's notes.
 * Used by the Inspector and Paint.
 */
export function Slider({ min, max, step, value, onChange }: { min: number; max: number; step: number; value: number; onChange: (value: number) => void }) {
  const trackRef = useRef<HTMLDivElement>(null)
  const percent = ((value - min) / (max - min)) * 100

  const valueFromClientX = (clientX: number) => {
    const rect = trackRef.current!.getBoundingClientRect()
    const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width))
    const raw = min + ratio * (max - min)
    const snapped = Math.round(raw / step) * step
    return Math.min(max, Math.max(min, snapped))
  }

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.currentTarget.setPointerCapture(event.pointerId)
    onChange(valueFromClientX(event.clientX))
  }
  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.buttons !== 1) return
    onChange(valueFromClientX(event.clientX))
  }

  return (
    <div
      ref={trackRef}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      className="relative h-4 w-full cursor-pointer touch-none"
    >
      <div className="absolute top-1/2 h-1 w-full -translate-y-1/2 rounded-full bg-overlay/10" />
      <div className="absolute top-1/2 h-1 -translate-y-1/2 rounded-full bg-accent" style={{ width: `${percent}%` }} />
      <div
        className="absolute top-1/2 h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent shadow"
        style={{ left: `${percent}%` }}
      />
    </div>
  )
}
