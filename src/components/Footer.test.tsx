// @vitest-environment jsdom

import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import Footer from './Footer'

describe('Footer navigation', () => {
  afterEach(cleanup)

  it('renders the bundled Commandeer mark without a broken root-relative URL', () => {
    const { container } = render(<Footer selectedItem={null} primaryAction={null} />)

    const navigation = container.querySelector('[data-footer-navigation]')
    expect(navigation).not.toBeNull()
    expect(container.querySelector('img[src="/favicon.svg"]')).toBeNull()
    expect(navigation?.querySelector('img')?.getAttribute('src')).toMatch(/^data:image\/svg\+xml/)
  })

  it('keeps the bundled mark beside a navigation title when the step has no icon', () => {
    const { container } = render(<Footer selectedItem={null} primaryAction={null} navigationTitle="Tools" />)
    const navigation = container.querySelector('[data-footer-navigation]')

    expect(navigation?.textContent).toContain('Tools')
    expect(navigation?.querySelector('img')).not.toBeNull()
  })

  it('replaces an invalid legacy navigation icon instead of rendering a question mark', () => {
    const { container } = render(
      <Footer selectedItem={null} primaryAction={null} navigationTitle="Legacy item" navigationIcon="?" />,
    )
    const navigation = container.querySelector('[data-footer-navigation]')

    expect(navigation?.textContent).toBe('Legacy item')
    expect(navigation?.querySelector('img')?.getAttribute('src')).toMatch(/^data:image\/svg\+xml/)
  })
})
