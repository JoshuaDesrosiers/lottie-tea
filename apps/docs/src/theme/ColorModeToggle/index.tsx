/**
 * The navbar's light / dark switch is the `theme` Toasty Lite animation:
 * the sun's rays retract and it morphs into a moon. The site's colour mode
 * drives the animation's `dark` input; clicking the button flips the mode.
 */
import React, { type ReactNode } from 'react'
import useIsBrowser from '@docusaurus/useIsBrowser'
import { useColorMode } from '@docusaurus/theme-common'
import type { Props } from '@theme/ColorModeToggle'
import { Lite } from '@site/src/components/Lite'

export default function ColorModeToggle({ className, buttonClassName }: Props): ReactNode {
  const isBrowser = useIsBrowser()
  const { colorMode, setColorMode } = useColorMode()
  const dark = colorMode === 'dark'
  return (
    <div className={className} style={{ display: 'flex' }}>
      <button
        type="button"
        className={`clean-btn toasty-icon-button ${buttonClassName ?? ''}`}
        onClick={() => setColorMode(dark ? 'light' : 'dark')}
        disabled={!isBrowser}
        title={dark ? 'Switch to light mode' : 'Switch to dark mode'}
        aria-label={dark ? 'Switch to light mode' : 'Switch to dark mode'}
      >
        <Lite src="theme" inputs={{ dark }} style={{ width: 30, height: 30, pointerEvents: 'none' }} />
      </button>
    </div>
  )
}
