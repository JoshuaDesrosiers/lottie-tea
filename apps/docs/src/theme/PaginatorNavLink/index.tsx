/**
 * Previous / next page links, with the `arrow` Toasty Lite animation
 * driven by the link's hover (the arrow is mirrored for "previous").
 */
import React, { useState, type ReactNode } from 'react'
import clsx from 'clsx'
import Link from '@docusaurus/Link'
import type { Props } from '@theme/PaginatorNavLink'
import { Lite } from '@site/src/components/Lite'

export default function PaginatorNavLink(props: Props): ReactNode {
  const { permalink, title, subLabel, isNext } = props
  const [hover, setHover] = useState(false)
  return (
    <Link
      className={clsx('pagination-nav__link', isNext ? 'pagination-nav__link--next' : 'pagination-nav__link--prev', 'toasty-pager')}
      to={permalink}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onFocus={() => setHover(true)}
      onBlur={() => setHover(false)}
    >
      <span className="toasty-pager-text">
        {subLabel && <div className="pagination-nav__sublabel">{subLabel}</div>}
        <div className="pagination-nav__label">{title}</div>
      </span>
      <Lite src="arrow" ink="auto" inputs={{ hover }} style={{ width: 28, height: 28, flex: 'none', transform: isNext ? undefined : 'scaleX(-1)', order: isNext ? 1 : -1 }} />
    </Link>
  )
}
