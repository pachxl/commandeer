import { describe, expect, it } from 'vitest'
import { normalizeAbsolutePath, parseSearchRoots } from './configPaths'

describe('configured filesystem paths', () => {
  it('accepts absolute Unix paths and removes matching quotes', () => {
    expect(normalizeAbsolutePath("  '/Users/alex/My Scripts'  ", 'unix')).toBe('/Users/alex/My Scripts')
    expect(normalizeAbsolutePath('~/Scripts', 'unix')).toBeNull()
    expect(normalizeAbsolutePath('relative/scripts', 'unix')).toBeNull()
  })

  it('accepts drive and UNC paths on Windows', () => {
    expect(normalizeAbsolutePath('C:\\Users\\alex\\Scripts', 'windows')).toBe('C:\\Users\\alex\\Scripts')
    expect(normalizeAbsolutePath('\\\\server\\share\\commands', 'windows')).toBe('\\\\server\\share\\commands')
    expect(normalizeAbsolutePath('scripts\\local', 'windows')).toBeNull()
  })

  it('parses newline-separated roots while dropping blanks and duplicates', () => {
    expect(parseSearchRoots('/Users/alex/Desktop\n\n/Users/alex/Projects\n/Users/alex/Desktop', 'unix')).toEqual({
      paths: ['/Users/alex/Desktop', '/Users/alex/Projects'],
      invalid: [],
    })
  })

  it('reports invalid roots and deduplicates Windows paths case-insensitively', () => {
    expect(parseSearchRoots('C:\\Projects\nc:\\projects\nrelative\nD:\\Work', 'windows')).toEqual({
      paths: ['C:\\Projects', 'D:\\Work'],
      invalid: ['relative'],
    })
  })
})
