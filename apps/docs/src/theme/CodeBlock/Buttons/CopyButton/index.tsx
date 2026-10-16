/**
 * Code blocks' copy button is the `copy` Toasty Lite animation: the two
 * sheets draw out and a check draws in (its `copy` trigger), then it
 * returns on its own.
 */
import React, { useCallback, useRef, type ReactNode } from 'react'
import clsx from 'clsx'
import { useCodeBlockContext } from '@docusaurus/theme-common/internal'
import Button from '@theme/CodeBlock/Buttons/Button'
import type { Props } from '@theme/CodeBlock/Buttons/CopyButton'
import { Lite, type LiteHandle } from '@site/src/components/Lite'

export default function CopyButton({ className }: Props): ReactNode {
  const {
    metadata: { code },
  } = useCodeBlockContext()
  const icon = useRef<LiteHandle | null>(null)
  const copy = useCallback(() => {
    void navigator.clipboard?.writeText(code).then(() => icon.current?.fire('copy'))
  }, [code])
  return (
    <Button aria-label="Copy code to clipboard" title="Copy" className={clsx(className, 'toasty-copy')} onClick={copy}>
      <Lite ref={icon} src="copy" ink="auto" style={{ width: 22, height: 22, pointerEvents: 'none' }} />
    </Button>
  )
}
