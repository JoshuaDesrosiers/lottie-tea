/**
 * `ctx.net` — port-based multiplayer (host on a port, others join by
 * address), the game side of `crates/an1mbyt3-net`'s relay server.
 *
 * - **Host**: `ctx.net.host({ port })` starts the relay inside the desktop
 *   editor (Tauri command `net_host`), then joins it over loopback like any
 *   other player. The first player in a room is the *host player*
 *   (`ctx.net.isHost`); if they leave, the next-oldest takes over.
 * - **Join**: `ctx.net.join('192.168.1.20:7777')` from any copy of the game —
 *   the editor or an exported HTML game (browsers can connect, they just
 *   can't listen; a standalone `an1mbyt3-server` can host for them).
 * - **Talk**: `ctx.net.send(data, to?)` sends any JSON value to everyone
 *   else (or one player). Each frame, `ctx.net.messages` holds what arrived
 *   since the last frame (`{ from, data }`) and `ctx.net.events` the
 *   joins / leaves / host changes — read them like `ctx.collisions`.
 *
 * One connection per app (a module singleton, like `gameStorage`); it
 * survives `ctx.loadScene` and is closed when Playground stops.
 */
import { invoke } from '@tauri-apps/api/core'

export const DEFAULT_NET_PORT = 7777

export type NetStatus = 'offline' | 'connecting' | 'online' | 'error'
export interface NetPlayer {
  id: number
  name: string
}
export interface NetMessage {
  from: number
  data: unknown
}
export type NetEvent =
  | { type: 'connected'; id: number }
  | { type: 'disconnected' }
  | { type: 'join'; id: number; name: string }
  | { type: 'leave'; id: number; name: string }
  | { type: 'host'; id: number }

/** What scripts see as `ctx.net`. */
export interface ScriptNet {
  /** `'offline'` → `'connecting'` → `'online'`; `'error'` (see `error`) if
   * hosting / joining failed or the connection dropped. */
  readonly status: NetStatus
  readonly error: string | null
  /** Your player id once online. */
  readonly me: number | null
  /** The host player's id, and whether that's you. */
  readonly hostId: number | null
  readonly isHost: boolean
  /** Everyone in the room (you included), oldest first. */
  readonly players: readonly NetPlayer[]
  /** Messages that arrived since the last frame. */
  readonly messages: readonly NetMessage[]
  /** Joins / leaves / host changes / (dis)connection since the last frame. */
  readonly events: readonly NetEvent[]
  /** While you host: the port, and the addresses others type to join. */
  readonly hosting: { port: number; addresses: string[] } | null
  /** Whether this copy of the game can host (the desktop editor can; a
   * browser can only join). */
  readonly canHost: boolean
  /** Host a game on `port` (default 7777) and join it as `name`. */
  host(options?: { port?: number; name?: string }): void
  /** Join `address` — `"192.168.1.20:7777"`, `"localhost"` (port 7777)… */
  join(address: string, options?: { name?: string }): void
  /** Disconnect (and stop hosting). */
  leave(): void
  /** Send any JSON value to everyone else, or just player `to`. */
  send(data: unknown, to?: number): void
  /** A player's name (`'?'` if unknown). */
  nameOf(id: number): string
}

const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export class NetClient implements ScriptNet {
  status: NetStatus = 'offline'
  error: string | null = null
  me: number | null = null
  hostId: number | null = null
  players: NetPlayer[] = []
  hosting: { port: number; addresses: string[] } | null = null
  messages: NetMessage[] = []
  events: NetEvent[] = []
  private socket: WebSocket | null = null
  private inbox: NetMessage[] = []
  private pendingEvents: NetEvent[] = []
  /** Bumped on every connect / leave, so stale async results are ignored. */
  private session = 0

  get isHost(): boolean {
    return this.me !== null && this.me === this.hostId
  }

  get canHost(): boolean {
    return isTauri()
  }

  /** Called once per frame before scripts run: this frame's messages and
   * events are what arrived since the previous call. */
  beginFrame(): void {
    this.messages = this.inbox
    this.events = this.pendingEvents
    this.inbox = []
    this.pendingEvents = []
  }

  nameOf(id: number): string {
    return this.players.find((p) => p.id === id)?.name ?? '?'
  }

  host(options?: { port?: number; name?: string }): void {
    const port = Math.round(options?.port ?? DEFAULT_NET_PORT)
    const name = options?.name ?? 'Host'
    this.leave()
    const session = this.session
    if (!this.canHost) {
      this.fail("Hosting needs the desktop app (or run an1mbyt3-server) — this copy can only join")
      return
    }
    this.status = 'connecting'
    invoke<{ port: number; addresses: string[] }>('net_host', { port })
      .then((hosting) => {
        if (session !== this.session) return
        this.hosting = hosting
        this.connect(`ws://127.0.0.1:${hosting.port}`, name, session)
      })
      .catch((e) => {
        if (session === this.session) this.fail(String(e))
      })
  }

  join(address: string, options?: { name?: string }): void {
    this.leave()
    let url = address.trim()
    if (!url) {
      this.fail('Type an address to join, like 192.168.1.20:7777')
      return
    }
    if (!/^wss?:\/\//.test(url)) url = 'ws://' + url
    // No port given: the default one.
    if (!/:\d+(\/|$)/.test(url.replace(/^wss?:\/\//, ''))) url = url.replace(/\/?$/, `:${DEFAULT_NET_PORT}`)
    this.status = 'connecting'
    this.connect(url, options?.name ?? 'Player', this.session)
  }

  leave(): void {
    this.session++
    const socket = this.socket
    this.socket = null
    if (socket) {
      socket.onclose = socket.onmessage = socket.onerror = null
      try {
        socket.close()
      } catch {
        // already closed
      }
    }
    if (this.hosting && isTauri()) void invoke('net_stop').catch(() => {})
    const wasOnline = this.status === 'online'
    this.hosting = null
    this.status = 'offline'
    this.error = null
    this.me = this.hostId = null
    this.players = []
    if (wasOnline) this.pendingEvents.push({ type: 'disconnected' })
  }

  send(data: unknown, to?: number): void {
    if (this.status !== 'online' || !this.socket) return
    this.socket.send(JSON.stringify(to === undefined ? { t: 'send', data } : { t: 'send', data, to }))
  }

  /** Playground stopped: drop everything, including this frame's queue. */
  reset(): void {
    this.leave()
    this.inbox = []
    this.pendingEvents = []
    this.messages = []
    this.events = []
  }

  private fail(message: string): void {
    this.status = 'error'
    this.error = message
  }

  private connect(url: string, name: string, session: number): void {
    let socket: WebSocket
    try {
      socket = new WebSocket(url)
    } catch (e) {
      this.fail(`Can't connect to ${url}: ${e}`)
      return
    }
    this.socket = socket
    socket.onopen = () => socket.send(JSON.stringify({ t: 'hello', name }))
    socket.onerror = () => {
      if (session === this.session && this.status === 'connecting') this.fail(`Couldn't reach ${url.replace(/^ws:\/\//, '')} — is a game hosted there?`)
    }
    socket.onclose = () => {
      if (session !== this.session) return
      const was = this.status
      this.socket = null
      this.me = this.hostId = null
      this.players = []
      if (was === 'online') {
        this.pendingEvents.push({ type: 'disconnected' })
        this.fail('Disconnected from the host')
      } else if (was === 'connecting') {
        this.fail(this.error ?? `Couldn't reach ${url.replace(/^ws:\/\//, '')}`)
      }
    }
    socket.onmessage = (event) => {
      if (session !== this.session) return
      let m: { t?: string; id?: number; name?: string; host?: number | null; players?: NetPlayer[]; from?: number; data?: unknown }
      try {
        m = JSON.parse(String(event.data))
      } catch {
        return
      }
      switch (m.t) {
        case 'welcome':
          this.me = m.id ?? null
          this.hostId = m.host ?? null
          this.players = m.players ?? []
          this.status = 'online'
          this.error = null
          if (this.me !== null) this.pendingEvents.push({ type: 'connected', id: this.me })
          break
        case 'join':
          if (m.id !== undefined) {
            this.players = [...this.players.filter((p) => p.id !== m.id), { id: m.id, name: m.name ?? 'Player' }].sort((a, b) => a.id - b.id)
            this.pendingEvents.push({ type: 'join', id: m.id, name: m.name ?? 'Player' })
          }
          break
        case 'leave':
          if (m.id !== undefined) {
            const name = this.nameOf(m.id)
            this.players = this.players.filter((p) => p.id !== m.id)
            this.pendingEvents.push({ type: 'leave', id: m.id, name })
          }
          break
        case 'host':
          if (m.id !== undefined) {
            this.hostId = m.id
            this.pendingEvents.push({ type: 'host', id: m.id })
          }
          break
        case 'msg':
          if (m.from !== undefined) this.inbox.push({ from: m.from, data: m.data })
          break
      }
    }
  }
}

/** The *active* network connection (the game's `ctx.net`; live binding):
 * the editor's, or — around an embedded player's frame — that player's
 * own, so two games on one page don't share a room (`runtimeContext.ts`). */
export let gameNet = new NetClient()

/** Makes `next` the active connection; returns the one it replaced. */
export function swapGameNet(next: NetClient): NetClient {
  const previous = gameNet
  gameNet = next
  return previous
}
