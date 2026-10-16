// Everything `jsx live` code blocks can use without importing.
import React from 'react'
import { ToastyPlayer } from '@site/src/components/ToastyPlayer'
import { Lite } from '@site/src/components/Lite'

const ReactLiveScope: Record<string, unknown> = {
  React,
  ...React,
  ToastyPlayer,
  Lite,
}

export default ReactLiveScope
