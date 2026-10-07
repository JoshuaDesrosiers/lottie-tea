/**
 * `gifenc` ships no type declarations of its own (plain JS, see its
 * `package.json`) — this covers only the handful of exports `exportGif.ts`
 * actually calls, not the library's full surface.
 */
declare module 'gifenc' {
  export function quantize(rgba: Uint8Array | Uint8ClampedArray, maxColors: number, options?: Record<string, unknown>): number[][]

  export function applyPalette(rgba: Uint8Array | Uint8ClampedArray, palette: number[][], format?: string): Uint8Array

  export interface GIFEncoderWriteFrameOptions {
    transparent?: boolean
    transparentIndex?: number
    delay?: number
    palette?: number[][] | null
    repeat?: number
    colorDepth?: number
    dispose?: number
  }

  export interface GIFEncoderInstance {
    reset(): void
    finish(): void
    bytes(): Uint8Array
    bytesView(): Uint8Array
    writeFrame(index: Uint8Array, width: number, height: number, opts?: GIFEncoderWriteFrameOptions): void
  }

  export function GIFEncoder(opts?: { initialCapacity?: number; auto?: boolean }): GIFEncoderInstance
}
