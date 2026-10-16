// Everything `jsx live` code blocks can use without importing.
import React from 'react'
import { ToastyPlayer } from '@site/src/components/ToastyPlayer'

const ReactLiveScope: Record<string, unknown> = {
  React,
  ...React,
  ToastyPlayer,
}

export default ReactLiveScope
