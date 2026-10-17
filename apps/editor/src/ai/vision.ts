/**
 * On-device image AI for the Image Effects: **depth estimation** (for Depth
 * Map / Depth Blur) and **subject cut-out** (background removal), run in the
 * browser / WebView with transformers.js — no server, nothing uploaded.
 *
 * The library and the models load on first use only (when the author clicks
 * "Generate"), from jsDelivr and the Hugging Face hub, then stay cached by
 * the browser:
 *
 * - depth: `onnx-community/depth-anything-v2-small` (~25 MB)
 * - cut-out: `briaai/RMBG-1.4` (~45 MB)
 *
 * Inputs and outputs are encoded image bytes (PNG / JPEG in, PNG out), so
 * results go straight into the document as image assets.
 */

const TRANSFORMERS_URL = 'https://cdn.jsdelivr.net/npm/@huggingface/transformers@3.5.1'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Transformers = any
let lib: Promise<Transformers> | null = null
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const pipes = new Map<string, Promise<any>>()

function transformers(): Promise<Transformers> {
  lib ??= import(/* @vite-ignore */ TRANSFORMERS_URL).catch((err) => {
    lib = null
    throw new Error(`Couldn't load the AI library (needs internet the first time): ${err}`)
  })
  return lib
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function pipe(task: string, model: string, onProgress?: (msg: string) => void): Promise<any> {
  const key = `${task}:${model}`
  if (!pipes.has(key)) {
    const t = await transformers()
    const device = typeof navigator !== 'undefined' && 'gpu' in navigator ? 'webgpu' : 'wasm'
    pipes.set(
      key,
      t
        .pipeline(task, model, {
          device,
          dtype: device === 'webgpu' ? 'fp32' : 'q8',
          progress_callback: (p: { status?: string; progress?: number; file?: string }) => {
            if (p.status === 'progress' && typeof p.progress === 'number') onProgress?.(`Downloading model… ${Math.round(p.progress)}%`)
          },
        })
        .catch((err: unknown) => {
          pipes.delete(key)
          throw err
        }),
    )
  }
  return pipes.get(key)!
}

async function toUrl(bytes: Uint8Array): Promise<string> {
  return URL.createObjectURL(new Blob([bytes.slice().buffer]))
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function rawToPng(raw: any): Promise<Uint8Array> {
  const blob: Blob = await raw.toBlob('image/png')
  return new Uint8Array(await blob.arrayBuffer())
}

/** A greyscale depth image (white = near) for `image`, same size. */
export async function estimateDepth(image: Uint8Array, onProgress?: (msg: string) => void): Promise<Uint8Array> {
  onProgress?.('Loading depth model…')
  const p = await pipe('depth-estimation', 'onnx-community/depth-anything-v2-small', onProgress)
  onProgress?.('Estimating depth…')
  const url = await toUrl(image)
  try {
    const out = await p(url)
    const depth = Array.isArray(out) ? out[0].depth : out.depth
    return await rawToPng(depth)
  } finally {
    URL.revokeObjectURL(url)
  }
}

/** `image` with its background removed (transparent), same size. */
export async function cutOutSubject(image: Uint8Array, onProgress?: (msg: string) => void): Promise<Uint8Array> {
  onProgress?.('Loading cut-out model…')
  const p = await pipe('background-removal', 'briaai/RMBG-1.4', onProgress)
  onProgress?.('Finding the subject…')
  const url = await toUrl(image)
  try {
    const out = await p(url)
    return await rawToPng(Array.isArray(out) ? out[0] : out)
  } finally {
    URL.revokeObjectURL(url)
  }
}
