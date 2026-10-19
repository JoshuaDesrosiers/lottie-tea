/**
 * Sound playback for Playground and exported games — the host half of the
 * engine's audio support (`an1mbyt3_core::audio_asset`): the document only
 * stores each sound's encoded bytes; this decodes them with Web Audio and
 * plays them.
 *
 * - `play(editor, id, options)` — a clip (an `AudioId`), optionally looped,
 *   pitched, panned, or tied to a node for 2D spatial sound (`update` pans
 *   it by the node's position relative to the listener and fades it with
 *   distance).
 * - `tone(options)` — a synthesized beep / zap / blip (no asset needed):
 *   oscillator + pitch slide + envelope, for quick game feedback.
 * - `stopAll()` — leaving Playground, starting a new session.
 *
 * Browsers only let audio start after a user gesture, so the context is
 * created / resumed on the first pointer or key press; sounds requested
 * before that simply start once it's running.
 */
import type { OpaqueId } from '../ipc/document'
import type { WasmEditor } from '../ipc/wasmEditor'

export interface PlayOptions {
  /** 0..1 (above 1 amplifies). Default 1. */
  volume?: number
  /** Playback rate: 1 = normal, 2 = an octave up. Default 1. */
  pitch?: number
  loop?: boolean
  /** -1 (left) .. 1 (right), for non-spatial sounds. */
  pan?: number
  /** The node playing it (`"idx:version"`): `stopNodeSounds` can stop it. */
  node?: string
  /** Pan / fade by the node's position relative to the listener (`update`). */
  spatial?: boolean
  /** Spatial falloff distance (world units); default 600. */
  range?: number
}

export interface ToneOptions {
  /** Start frequency, Hz. Default 440. */
  freq?: number
  /** End frequency (a slide), Hz. Default = `freq`. */
  to?: number
  /** 'sine' | 'square' | 'sawtooth' | 'triangle' | 'noise'. Default 'square'. */
  type?: OscillatorType | 'noise'
  /** Seconds. Default 0.15. */
  duration?: number
  /** 0..1. Default 0.3. */
  volume?: number
  /** Fade-in / fade-out seconds. Defaults 0.005 / 60% of the duration. */
  attack?: number
  release?: number
  pan?: number
}

/** A playing sound. */
export interface SoundHandle {
  stop(): void
  setVolume(volume: number): void
  setPitch(pitch: number): void
  readonly playing: boolean
}

interface Voice extends SoundHandle {
  node?: string
  spatial: boolean
  range: number
  baseVolume: number
  gain: GainNode
  panner: StereoPannerNode
  ended: boolean
}

const keyOf = (id: OpaqueId) => `${id.idx}:${id.version}`

/** Spatial mix for a source at `dx, dy` from the listener: pan and gain. */
export function spatialMix(dx: number, dy: number, range: number): { pan: number; gain: number } {
  const r = Math.max(1, range)
  const d = Math.hypot(dx, dy)
  return { pan: Math.max(-1, Math.min(1, dx / (r * 0.5))), gain: Math.max(0, Math.min(1, 1 - d / r)) }
}

export class AudioEngine {
  private ctx: AudioContext | null = null
  private master: GainNode | null = null
  private masterVolume = 1
  private buffers = new Map<string, Promise<AudioBuffer | null>>()
  private voices = new Set<Voice>()
  private unlocked = false
  /** The page-wide "first gesture" listener (removed by `dispose`). */
  private unlock: (() => void) | null = null

  constructor() {
    if (typeof window === 'undefined') return
    // Autoplay rules: create / resume on the first gesture.
    const unlock = () => {
      this.unlocked = true
      const ctx = this.context()
      if (ctx && ctx.state === 'suspended') void ctx.resume()
    }
    this.unlock = unlock
    window.addEventListener('pointerdown', unlock, { capture: true })
    window.addEventListener('keydown', unlock, { capture: true })
  }

  /** Stops every sound, closes the audio context and unhooks the gesture
   * listener — an embedded player's own engine, when the player is
   * destroyed (the editor's lives as long as the page). */
  dispose(): void {
    this.stopAll()
    if (this.unlock && typeof window !== 'undefined') {
      window.removeEventListener('pointerdown', this.unlock, { capture: true })
      window.removeEventListener('keydown', this.unlock, { capture: true })
    }
    this.unlock = null
    void this.ctx?.close().catch(() => undefined)
    this.ctx = null
    this.master = null
  }

  /** The audio context (created lazily; `null` where Web Audio is missing). */
  private context(): AudioContext | null {
    if (this.ctx) return this.ctx
    if (typeof AudioContext === 'undefined') return null
    this.ctx = new AudioContext()
    this.master = this.ctx.createGain()
    this.master.gain.value = this.masterVolume
    this.master.connect(this.ctx.destination)
    if (this.unlocked && this.ctx.state === 'suspended') void this.ctx.resume()
    return this.ctx
  }

  /** Decodes a sound once (cached by id). */
  private load(editor: WasmEditor, id: OpaqueId): Promise<AudioBuffer | null> {
    const key = keyOf(id)
    let pending = this.buffers.get(key)
    if (!pending) {
      const ctx = this.context()
      if (!ctx) return Promise.resolve(null)
      const bytes = editor.audio_bytes(id) as Uint8Array
      pending = bytes.length ? ctx.decodeAudioData(bytes.slice().buffer).catch((err) => (console.warn('audio: could not decode a sound', err), null)) : Promise.resolve(null)
      this.buffers.set(key, pending)
    }
    return pending
  }

  /** Forgets decoded sounds (after an asset was replaced / deleted). */
  invalidate(): void {
    this.buffers.clear()
  }

  private voice(ctx: AudioContext, volume: number, pan: number, node: string | undefined, range: number, spatial = false): Voice {
    const gain = ctx.createGain()
    gain.gain.value = volume
    const panner = ctx.createStereoPanner()
    panner.pan.value = pan
    gain.connect(panner).connect(this.master!)
    const voice: Voice = {
      node,
      spatial,
      range,
      baseVolume: volume,
      gain,
      panner,
      ended: false,
      stop: () => {},
      setVolume: (v: number) => {
        voice.baseVolume = v
        if (!voice.spatial) gain.gain.value = v
      },
      setPitch: () => {},
      get playing() {
        return !voice.ended
      },
    }
    this.voices.add(voice)
    return voice
  }

  private finish(voice: Voice): void {
    if (voice.ended) return
    voice.ended = true
    this.voices.delete(voice)
    try {
      voice.panner.disconnect()
    } catch {
      /* already disconnected */
    }
  }

  /** Plays sound `id`. Returns a handle at once (it starts once decoded). */
  play(editor: WasmEditor, id: OpaqueId, options: PlayOptions = {}): SoundHandle | null {
    const ctx = this.context()
    if (!ctx) return null
    const voice = this.voice(ctx, options.volume ?? 1, options.pan ?? 0, options.node, options.range ?? 600, !!options.spatial && !!options.node)
    let source: AudioBufferSourceNode | null = null
    let pitch = options.pitch ?? 1
    let stopped = false
    voice.stop = () => {
      stopped = true
      try {
        source?.stop()
      } catch {
        /* not started */
      }
      this.finish(voice)
    }
    voice.setPitch = (p: number) => {
      pitch = p
      if (source) source.playbackRate.value = p
    }
    void this.load(editor, id).then((buffer) => {
      if (stopped || !buffer) {
        if (!buffer) this.finish(voice)
        return
      }
      source = ctx.createBufferSource()
      source.buffer = buffer
      source.loop = !!options.loop
      source.playbackRate.value = pitch
      source.connect(voice.gain)
      source.onended = () => this.finish(voice)
      source.start()
    })
    return voice
  }

  /** A synthesized sound effect (see `ToneOptions`). */
  tone(options: ToneOptions = {}): SoundHandle | null {
    const ctx = this.context()
    if (!ctx) return null
    const duration = Math.max(0.01, options.duration ?? 0.15)
    const volume = options.volume ?? 0.3
    const attack = Math.max(0.001, options.attack ?? 0.005)
    const release = Math.max(0.001, options.release ?? duration * 0.6)
    const voice = this.voice(ctx, 1, options.pan ?? 0, undefined, 1)
    const t0 = ctx.currentTime
    const env = ctx.createGain()
    env.gain.setValueAtTime(0, t0)
    env.gain.linearRampToValueAtTime(volume, t0 + attack)
    env.gain.setValueAtTime(volume, Math.max(t0 + attack, t0 + duration - release))
    env.gain.linearRampToValueAtTime(0, t0 + duration)
    env.connect(voice.gain)
    let source: AudioScheduledSourceNode
    if (options.type === 'noise') {
      const frames = Math.ceil(ctx.sampleRate * duration)
      const buffer = ctx.createBuffer(1, frames, ctx.sampleRate)
      const data = buffer.getChannelData(0)
      for (let i = 0; i < frames; i++) data[i] = Math.random() * 2 - 1
      const noise = ctx.createBufferSource()
      noise.buffer = buffer
      source = noise
    } else {
      const osc = ctx.createOscillator()
      osc.type = options.type ?? 'square'
      const freq = options.freq ?? 440
      osc.frequency.setValueAtTime(freq, t0)
      if (options.to !== undefined && options.to !== freq) osc.frequency.exponentialRampToValueAtTime(Math.max(1, options.to), t0 + duration)
      source = osc
    }
    source.connect(env)
    source.onended = () => this.finish(voice)
    source.start(t0)
    source.stop(t0 + duration + 0.02)
    voice.stop = () => {
      try {
        source.stop()
      } catch {
        /* done */
      }
      this.finish(voice)
    }
    return voice
  }

  /** Re-mixes node-tied (spatial) sounds for the listener at `(lx, ly)`. */
  update(lx: number, ly: number, positionOf: (node: string) => { x: number; y: number } | null): void {
    for (const voice of this.voices) {
      if (!voice.spatial || !voice.node) continue
      const p = positionOf(voice.node)
      if (!p) continue
      const mix = spatialMix(p.x - lx, p.y - ly, voice.range)
      voice.panner.pan.value = mix.pan
      voice.gain.gain.value = voice.baseVolume * mix.gain
    }
  }

  /** Stops node-tied sounds (`node` given: just that node's). */
  stopNodeSounds(node?: string): void {
    for (const voice of [...this.voices]) if (voice.node && (node === undefined || voice.node === node)) voice.stop()
  }

  stopAll(): void {
    for (const voice of [...this.voices]) voice.stop()
  }

  setMasterVolume(volume: number): void {
    this.masterVolume = Math.max(0, volume)
    if (this.master) this.master.gain.value = this.masterVolume
  }
}

/**
 * The *active* audio engine (live binding): the editor's, or — around an
 * embedded player's frame — that player's own, so one game's "stop all
 * sounds" / new session doesn't silence another game on the same page
 * (`runtimeContext.ts`). Each engine opens its `AudioContext` lazily, on
 * its first sound, so silent embeds cost nothing.
 */
export let audio = new AudioEngine()

/** Makes `next` the active engine; returns the one it replaced. */
export function swapAudio(next: AudioEngine): AudioEngine {
  const previous = audio
  audio = next
  return previous
}
