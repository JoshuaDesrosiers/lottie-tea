/**
 * The navbar logo: Toasty the mascot (a Toasty Lite animation — it blinks,
 * and hops when you point at the logo) next to the wordmark. The link's
 * own hover drives the animation's `hover` input, the same pattern the
 * docs recommend for icons inside buttons and links.
 */
import React, { useState, type ReactNode } from 'react'
import Link from '@docusaurus/Link'
import useBaseUrl from '@docusaurus/useBaseUrl'
import type { Props } from '@theme/Logo'
import { Lite } from '@site/src/components/Lite'

export default function Logo(props: Props): ReactNode {
  const { imageClassName: _i, titleClassName, ...rest } = props
  const home = useBaseUrl('/')
  const [hover, setHover] = useState(false)
  return (
    <Link to={home} {...rest} className="navbar__brand toasty-brand" onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)} aria-label="Toasty docs home">
      <Lite src="toasty" inputs={{ hover }} style={{ width: 34, height: 34, pointerEvents: 'none' }} />
      <b className={titleClassName}>Toasty</b>
      <span className="toasty-brand-tag">Docs</span>
    </Link>
  )
}
