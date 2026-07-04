// Minimal-scroll replacement for Element.scrollIntoView({ block: 'nearest' }):
// WKWebView (macOS) interprets 'nearest' by recentering the element, which
// makes the selection jump to the middle of the viewport when paging through a
// long list. Computing scrollTop from the rects is deterministic across
// engines (Chromium on Windows/Linux, WebKit on macOS) and only scrolls the
// minimum needed to bring the element fully into view.
export function scrollToReveal(container: HTMLElement | null, el: HTMLElement | null) {
  if (!container || !el) return
  const cRect = container.getBoundingClientRect()
  const eRect = el.getBoundingClientRect()
  if (eRect.top < cRect.top) {
    container.scrollTop += eRect.top - cRect.top
  } else if (eRect.bottom > cRect.bottom) {
    container.scrollTop += eRect.bottom - cRect.bottom
  }
}
