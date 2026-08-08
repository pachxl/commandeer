import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react'

const VERTEX_SHADER = `#version 300 es
in vec2 a_position;
out vec2 v_uv;

void main() {
  v_uv = a_position * 0.5 + 0.5;
  gl_Position = vec4(a_position, 0.0, 1.0);
}
`

export const ONIX_FRAGMENT_SHADER = `#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 out_color;

uniform vec2 u_resolution;
uniform vec2 u_pointer;
uniform vec3 u_accent;
uniform float u_radius;
uniform float u_activity;
uniform float u_dpr;

float rounded_box_sdf(vec2 point, vec2 half_size, float radius) {
  vec2 q = abs(point) - half_size + vec2(radius);
  return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - radius;
}

float hash21(vec2 point) {
  vec3 p3 = fract(vec3(point.xyx) * 0.1031);
  p3 += dot(p3, p3.yzx + 33.33);
  return fract((p3.x + p3.y) * p3.z);
}

float gaussian(float value, float sigma) {
  float x = value / max(sigma, 0.001);
  return exp(-(x * x));
}

vec2 rounded_box_normal(vec2 point, vec2 half_size, float radius) {
  vec2 q = abs(point) - half_size + vec2(radius);
  vec2 side = mix(vec2(-1.0), vec2(1.0), step(vec2(0.0), point));
  vec2 corner = max(q, 0.0);
  float corner_length = length(corner);
  if (corner_length > 0.0001) return (corner / corner_length) * side;
  return q.x > q.y ? vec2(side.x, 0.0) : vec2(0.0, side.y);
}

void main() {
  vec2 half_resolution = u_resolution * 0.5;
  vec2 point = v_uv * u_resolution - half_resolution;
  // The SDF is inset only far enough to keep antialiasing inside the canvas.
  // Geometry is evaluated in CSS pixels so optical thickness stays identical
  // on 1x and Retina displays; only this inset is explicitly physical-pixel
  // sized. Radius and edge therefore remain coincident with CSS/native masks.
  float antialias_inset = 0.65 / u_dpr;
  vec2 half_size = max(half_resolution - vec2(antialias_inset), vec2(1.0));
  float radius = min(max(u_radius - antialias_inset, 1.0), min(half_size.x, half_size.y));
  float distance_to_shell = rounded_box_sdf(point, half_size, radius);

  vec2 normal = rounded_box_normal(point, half_size, radius);
  float antialias = max(fwidth(distance_to_shell), 0.55 / u_dpr);
  float inside = 1.0 - smoothstep(-antialias, antialias, distance_to_shell);
  float depth = max(-distance_to_shell, 0.0);
  vec2 surface_point = point - normal * distance_to_shell;

  vec2 pointer_position = (vec2(u_pointer.x, 1.0 - u_pointer.y) - 0.5) * u_resolution;
  vec2 from_pointer = surface_point - pointer_position;
  float light_distance = max(length(from_pointer), 0.001);
  vec2 surface_ray = from_pointer / light_distance;
  float facing = max(dot(normal, surface_ray), 0.0);
  vec2 tangent = vec2(-normal.y, normal.x);

  // Gate interaction to the closest short arc of the perimeter. This keeps a
  // pointer near one edge from illuminating the opposite edge of a tall panel.
  float pointer_depth = max(-rounded_box_sdf(pointer_position, half_size, radius), 0.0);
  float normal_gap = max(dot(from_pointer, normal), 0.0);
  float excess_gap = max(normal_gap - pointer_depth, 0.0);
  float along_rim = abs(dot(from_pointer, tangent));
  float nearest_gate = gaussian(excess_gap, 34.0);
  float arc_gate = gaussian(along_rim, 58.0);
  float depth_reach = 0.38 + 0.62 * exp(-pointer_depth / 180.0);
  float pointer_focus = u_activity * nearest_gate * arc_gate * depth_reach * smoothstep(0.16, 0.94, facing);

  float rim = gaussian(distance_to_shell, 0.78);
  float micro_rim = gaussian(distance_to_shell + 0.16, 0.34);
  float inner_rim = inside * gaussian(depth - 1.65, 1.3);
  float ambient_facing = max(dot(normal, normalize(vec2(-0.34, 0.94))), 0.0);
  float ambient_rim = rim * (0.034 + 0.076 * pow(ambient_facing, 18.0));
  float pointer_specular = micro_rim * pointer_focus * (0.14 + 0.22 * pow(facing, 18.0));
  float pointer_sheen = inner_rim * pointer_focus * pow(facing, 5.0) * 0.065;

  // A restrained red/cyan shear sits around a white micro-rim. It reads as
  // refraction without painting a uniform rainbow border.
  float red_band = gaussian(distance_to_shell + 0.06, 0.38);
  float blue_band = gaussian(distance_to_shell + 1.02, 0.5);
  float chroma_strength = 0.014 + pointer_focus * (0.06 + 0.03 * pow(facing, 8.0));
  vec3 dispersion = vec3(red_band, 0.2 * (red_band + blue_band), blue_band) * chroma_strength;

  // One folded caustic tracks the active perimeter instead of casting a broad
  // directional cone across the content field.
  float perimeter_phase = atan(normal.y, normal.x);
  float liquid_phase = perimeter_phase * 2.35 + dot(surface_point, vec2(0.018, -0.013));
  float fold_depth = 6.7 + sin(liquid_phase) * 0.82 + sin(liquid_phase * 2.17 + 1.4) * 0.28;
  float caustic_fold = gaussian(depth - fold_depth, 0.72) + gaussian(depth - (fold_depth + 1.75), 1.85) * 0.22;
  float caustic = inside * pointer_focus * pow(facing, 4.0) * caustic_fold;

  // Beer-Lambert-like absorption leaves the refractive rim clear, reaches a
  // deep smoked black at capsule centre, and settles at 72% in a tall panel.
  float absorption = 1.0 - exp(-depth * 0.075);
  float material_alpha = mix(0.16, 0.72, pow(absorption, 0.82));
  float fill_alpha = inside * material_alpha;
  float dither = (hash21(gl_FragCoord.xy) - 0.5) / 255.0;
  vec3 base = vec3(0.005, 0.007, 0.012) + u_accent * 0.006 + vec3(dither);
  vec3 premultiplied = base * fill_alpha;

  premultiplied += vec3(0.72, 0.78, 0.87) * ambient_rim;
  premultiplied += vec3(1.0, 0.985, 0.96) * pointer_specular;
  premultiplied += mix(vec3(0.78, 0.87, 1.0), u_accent, 0.18) * pointer_sheen;
  premultiplied += mix(vec3(0.76, 0.88, 1.0), u_accent, 0.12) * caustic * 0.16;
  premultiplied += dispersion;

  float spectral_peak = max(dispersion.r, max(dispersion.g, dispersion.b));
  float optical_alpha =
    ambient_rim + pointer_specular + pointer_sheen * 0.5 + caustic * 0.12 + spectral_peak * 0.65;
  float alpha = clamp(max(fill_alpha, optical_alpha), 0.0, 0.985);
  premultiplied = min(premultiplied, vec3(alpha));
  out_color = vec4(premultiplied, alpha);
}
`

const RESTING_POINTER: readonly [number, number] = [0.16, 0.04]
const FALLBACK_ACCENT: readonly [number, number, number] = [0.47, 0.63, 0.96]
const POINTER_SETTLE = 0.16
const ACTIVITY_SETTLE = 0.18
const SETTLE_EPSILON = 0.0005
const SHAPE_TRANSITION_MS = 170
const MIN_RENDER_SCALE = 2
const MAX_DPR = 2

export interface OpticalRenderMetrics {
  logicalWidth: number
  logicalHeight: number
  renderScale: number
  pixelWidth: number
  pixelHeight: number
}

export function getOpticalRenderMetrics(width: number, height: number, devicePixelRatio: number): OpticalRenderMetrics {
  const sourceScale = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1
  const renderScale = Math.min(MAX_DPR, Math.max(MIN_RENDER_SCALE, sourceScale))
  return {
    logicalWidth: width,
    logicalHeight: height,
    renderScale,
    pixelWidth: Math.max(1, Math.round(width * renderScale)),
    pixelHeight: Math.max(1, Math.round(height * renderScale)),
  }
}

export type OnixOpticsMode = 'css' | 'webgl'

export interface UseOnixOpticsOptions {
  compact: boolean
  radius?: number
}

export interface OnixOpticsBinding {
  canvasRef: RefObject<HTMLCanvasElement>
  layerRef: RefObject<HTMLDivElement>
  mode: OnixOpticsMode
  reducedMotion: boolean
  reducedTransparency: boolean
  forcedColors: boolean
}

interface OpticalRenderer {
  resize: (width: number, height: number, dpr: number) => void
  draw: (
    pointer: readonly [number, number],
    radius: number,
    accent: readonly [number, number, number],
    activity: number,
  ) => void
  dispose: () => void
}

function useMediaPreference(query: string): boolean {
  const [matches, setMatches] = useState(() => {
    try {
      return (
        typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia(query).matches
      )
    } catch {
      return false
    }
  })

  useEffect(() => {
    if (typeof window.matchMedia !== 'function') return
    let media: MediaQueryList
    try {
      media = window.matchMedia(query)
    } catch {
      return
    }
    const update = () => setMatches(media.matches)
    update()
    if (typeof media.addEventListener === 'function') {
      media.addEventListener('change', update)
      return () => media.removeEventListener('change', update)
    }
    media.addListener(update)
    return () => media.removeListener(update)
  }, [query])

  return matches
}

function compileShader(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type)
  if (!shader) throw new Error('Unable to allocate an Onix optical shader')
  gl.shaderSource(shader, source)
  gl.compileShader(shader)
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const message = gl.getShaderInfoLog(shader) ?? 'Unknown shader compilation failure'
    gl.deleteShader(shader)
    throw new Error(message)
  }
  return shader
}

function requiredUniform(gl: WebGL2RenderingContext, program: WebGLProgram, name: string): WebGLUniformLocation {
  const location = gl.getUniformLocation(program, name)
  if (!location) throw new Error(`Missing Onix optical uniform: ${name}`)
  return location
}

function createRenderer(canvas: HTMLCanvasElement, gl: WebGL2RenderingContext): OpticalRenderer {
  const vertexShader = compileShader(gl, gl.VERTEX_SHADER, VERTEX_SHADER)
  const fragmentShader = compileShader(gl, gl.FRAGMENT_SHADER, ONIX_FRAGMENT_SHADER)
  const program = gl.createProgram()
  if (!program) throw new Error('Unable to allocate the Onix optical program')
  gl.attachShader(program, vertexShader)
  gl.attachShader(program, fragmentShader)
  gl.linkProgram(program)
  gl.deleteShader(vertexShader)
  gl.deleteShader(fragmentShader)
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    const message = gl.getProgramInfoLog(program) ?? 'Unknown shader link failure'
    gl.deleteProgram(program)
    throw new Error(message)
  }

  const position = gl.getAttribLocation(program, 'a_position')
  if (position < 0) {
    gl.deleteProgram(program)
    throw new Error('Missing Onix optical vertex position')
  }

  const buffer = gl.createBuffer()
  const vertexArray = gl.createVertexArray()
  if (!buffer || !vertexArray) {
    if (buffer) gl.deleteBuffer(buffer)
    if (vertexArray) gl.deleteVertexArray(vertexArray)
    gl.deleteProgram(program)
    throw new Error('Unable to allocate Onix optical geometry')
  }

  gl.bindVertexArray(vertexArray)
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer)
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW)
  gl.enableVertexAttribArray(position)
  gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0)
  gl.bindVertexArray(null)

  const resolutionUniform = requiredUniform(gl, program, 'u_resolution')
  const pointerUniform = requiredUniform(gl, program, 'u_pointer')
  const accentUniform = requiredUniform(gl, program, 'u_accent')
  const radiusUniform = requiredUniform(gl, program, 'u_radius')
  const activityUniform = requiredUniform(gl, program, 'u_activity')
  const dprUniform = requiredUniform(gl, program, 'u_dpr')
  let devicePixelRatio = 1
  let logicalWidth = 1
  let logicalHeight = 1

  gl.enable(gl.BLEND)
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA)

  return {
    resize(width, height, dpr) {
      devicePixelRatio = dpr
      logicalWidth = width
      logicalHeight = height
      const { pixelWidth, pixelHeight } = getOpticalRenderMetrics(width, height, dpr)
      if (canvas.width !== pixelWidth) canvas.width = pixelWidth
      if (canvas.height !== pixelHeight) canvas.height = pixelHeight
      gl.viewport(0, 0, pixelWidth, pixelHeight)
    },
    draw(pointer, radius, accent, activity) {
      gl.clearColor(0, 0, 0, 0)
      gl.clear(gl.COLOR_BUFFER_BIT)
      gl.useProgram(program)
      gl.bindVertexArray(vertexArray)
      gl.uniform2f(resolutionUniform, logicalWidth, logicalHeight)
      gl.uniform2f(pointerUniform, pointer[0], pointer[1])
      gl.uniform3f(accentUniform, accent[0], accent[1], accent[2])
      gl.uniform1f(radiusUniform, radius)
      gl.uniform1f(activityUniform, activity)
      gl.uniform1f(dprUniform, devicePixelRatio)
      gl.drawArrays(gl.TRIANGLES, 0, 3)
      gl.bindVertexArray(null)
    },
    dispose() {
      gl.deleteBuffer(buffer)
      gl.deleteVertexArray(vertexArray)
      gl.deleteProgram(program)
    },
  }
}

function parseAccent(value: string): readonly [number, number, number] {
  const color = value.trim()
  const shortHex = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(color)
  if (shortHex) {
    return shortHex.slice(1, 4).map(channel => parseInt(channel + channel, 16) / 255) as unknown as readonly [
      number,
      number,
      number,
    ]
  }
  const hex = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})(?:[0-9a-f]{2})?$/i.exec(color)
  if (hex) {
    return hex.slice(1, 4).map(channel => parseInt(channel, 16) / 255) as unknown as readonly [number, number, number]
  }
  const rgb = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)/i.exec(color)
  if (rgb) {
    return [Number(rgb[1]) / 255, Number(rgb[2]) / 255, Number(rgb[3]) / 255]
  }
  return FALLBACK_ACCENT
}

function effectiveRadius(layer: HTMLDivElement, compact: boolean, radius: number | undefined): number {
  const rect = layer.getBoundingClientRect()
  if (compact) return Math.max(1, Math.min(rect.width, rect.height) * 0.5)
  const logicalWidth = layer.offsetWidth || rect.width
  const zoom = logicalWidth > 0 ? rect.width / logicalWidth : 1
  return Math.max(1, Math.min((radius ?? 28) * zoom, Math.min(rect.width, rect.height) * 0.5))
}

function renderedRadius(layer: HTMLDivElement, fallback: number): number {
  const rect = layer.getBoundingClientRect()
  const logicalWidth = layer.offsetWidth || rect.width
  const zoom = logicalWidth > 0 ? rect.width / logicalWidth : 1
  const cssRadius = Number.parseFloat(getComputedStyle(layer).borderTopLeftRadius)
  if (!Number.isFinite(cssRadius)) return fallback
  return Math.max(1, Math.min(cssRadius * zoom, Math.min(rect.width, rect.height) * 0.5))
}

export function useOnixOptics({ compact, radius }: UseOnixOpticsOptions): OnixOpticsBinding {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const layerRef = useRef<HTMLDivElement>(null)
  const optionsRef = useRef({ compact, radius })
  optionsRef.current = { compact, radius }
  const invalidateRef = useRef<(() => void) | null>(null)
  const [mode, setMode] = useState<OnixOpticsMode>('css')
  const [contextGeneration, setContextGeneration] = useState(0)
  const reducedMotion = useMediaPreference('(prefers-reduced-motion: reduce)')
  const reducedTransparency = useMediaPreference('(prefers-reduced-transparency: reduce)')
  const forcedColors = useMediaPreference('(forced-colors: active)')

  useLayoutEffect(() => {
    invalidateRef.current?.()
  }, [compact, radius])

  useLayoutEffect(() => {
    const canvas = canvasRef.current
    const layer = layerRef.current
    if (!canvas || !layer) return

    const host = layer.parentElement
    let renderer: OpticalRenderer | null = null
    let animationFrame: number | null = null
    let currentPointer: [number, number] = [...RESTING_POINTER]
    let targetPointer: [number, number] = [...RESTING_POINTER]
    let currentRadius = effectiveRadius(layer, optionsRef.current.compact, optionsRef.current.radius)
    let targetRadius = currentRadius
    let currentActivity = 0
    let targetActivity = 0
    let shapeTransitionUntil = 0
    let accent = parseAccent(getComputedStyle(layer).getPropertyValue('--accent'))
    let dpr = getOpticalRenderMetrics(1, 1, window.devicePixelRatio || 1).renderScale
    let width = 0
    let height = 0

    const updateFallbackPointer = () => {
      layer.style.setProperty('--onix-pointer-x', `${(currentPointer[0] * 100).toFixed(2)}%`)
      layer.style.setProperty('--onix-pointer-y', `${(currentPointer[1] * 100).toFixed(2)}%`)
      layer.style.setProperty('--onix-pointer-activity', currentActivity.toFixed(3))
    }

    const draw = () => {
      updateFallbackPointer()
      renderer?.draw(currentPointer, currentRadius, accent, currentActivity)
    }

    const tick = (timestamp: number) => {
      animationFrame = null
      if (reducedMotion) {
        currentPointer = [...RESTING_POINTER]
        currentRadius = targetRadius
        currentActivity = 0
        draw()
        return
      }

      const shapeAnimating = timestamp < shapeTransitionUntil
      if (shapeAnimating) {
        currentRadius = renderedRadius(layer, currentRadius)
      } else if (shapeTransitionUntil > 0) {
        shapeTransitionUntil = 0
        currentRadius = targetRadius
      }

      currentPointer[0] += (targetPointer[0] - currentPointer[0]) * POINTER_SETTLE
      currentPointer[1] += (targetPointer[1] - currentPointer[1]) * POINTER_SETTLE
      currentActivity += (targetActivity - currentActivity) * ACTIVITY_SETTLE
      draw()

      const pointerDelta =
        Math.abs(targetPointer[0] - currentPointer[0]) + Math.abs(targetPointer[1] - currentPointer[1])
      const activityDelta = Math.abs(targetActivity - currentActivity)
      if (pointerDelta > SETTLE_EPSILON || activityDelta > SETTLE_EPSILON || shapeAnimating) {
        animationFrame = requestAnimationFrame(tick)
      } else {
        currentPointer = [...targetPointer]
        currentRadius = targetRadius
        currentActivity = targetActivity
        draw()
      }
    }

    const scheduleDraw = () => {
      if (animationFrame == null) animationFrame = requestAnimationFrame(tick)
    }

    const invalidateShape = () => {
      targetRadius = effectiveRadius(layer, optionsRef.current.compact, optionsRef.current.radius)
      if (reducedMotion) {
        currentRadius = targetRadius
        shapeTransitionUntil = 0
      } else {
        // Read the browser's live interpolated border radius on each frame.
        // That keeps the spectral SDF rim coincident with the CSS mask while
        // the native window and glass surface bloom over the same 150ms.
        currentRadius = renderedRadius(layer, currentRadius)
        shapeTransitionUntil = performance.now() + SHAPE_TRANSITION_MS
      }
      scheduleDraw()
    }
    invalidateRef.current = invalidateShape

    const resize = () => {
      const rect = layer.getBoundingClientRect()
      if (rect.width <= 0 || rect.height <= 0) return
      const hadSize = width > 0 && height > 0
      width = rect.width
      height = rect.height
      // Supersample the thin optical rim on 1x desktop monitors. The canvas is
      // tiny relative to the app, and 2x prevents the refractive/spectral edge
      // from becoming a visibly coarse one-pixel staircase.
      dpr = getOpticalRenderMetrics(width, height, window.devicePixelRatio || 1).renderScale
      renderer?.resize(width, height, dpr)
      targetRadius = effectiveRadius(layer, optionsRef.current.compact, optionsRef.current.radius)
      if (!hadSize || reducedMotion || performance.now() >= shapeTransitionUntil) {
        currentRadius = targetRadius
      } else {
        currentRadius = renderedRadius(layer, currentRadius)
      }
      // Assigning canvas.width/height clears its backing store. Repaint in the
      // same layout phase so a native capsule/panel resize never exposes one
      // transparent or darker frame while waiting for the next RAF.
      draw()
      scheduleDraw()
    }

    const handlePointerMove = (event: PointerEvent) => {
      if (reducedMotion || forcedColors) return
      const rect = layer.getBoundingClientRect()
      if (rect.width <= 0 || rect.height <= 0) return
      targetPointer = [
        Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width)),
        Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height)),
      ]
      targetActivity = 1
      scheduleDraw()
    }

    const handlePointerLeave = () => {
      targetPointer = [...RESTING_POINTER]
      targetActivity = 0
      scheduleDraw()
    }

    const handleContextLost = (event: Event) => {
      event.preventDefault()
      renderer = null
      setMode('css')
    }

    const handleContextRestored = () => setContextGeneration(generation => generation + 1)

    host?.addEventListener('pointermove', handlePointerMove)
    host?.addEventListener('pointerleave', handlePointerLeave)
    canvas.addEventListener('webglcontextlost', handleContextLost)
    canvas.addEventListener('webglcontextrestored', handleContextRestored)

    const resizeObserver = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(resize)
    resizeObserver?.observe(layer)

    const themeObserver =
      typeof MutationObserver === 'undefined'
        ? null
        : new MutationObserver(() => {
            accent = parseAccent(getComputedStyle(layer).getPropertyValue('--accent'))
            scheduleDraw()
          })
    themeObserver?.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['style', 'data-theme'],
    })

    if (!reducedTransparency && !forcedColors) {
      try {
        const gl = canvas.getContext('webgl2', {
          alpha: true,
          antialias: true,
          depth: false,
          premultipliedAlpha: true,
          preserveDrawingBuffer: false,
          stencil: false,
        })
        if (gl) {
          renderer = createRenderer(canvas, gl)
        }
      } catch (error) {
        console.warn('Onix WebGL optics unavailable; using the CSS material.', error)
      }
    }

    setMode(renderer ? 'webgl' : 'css')

    resize()

    return () => {
      if (invalidateRef.current === invalidateShape) invalidateRef.current = null
      if (animationFrame != null) cancelAnimationFrame(animationFrame)
      resizeObserver?.disconnect()
      themeObserver?.disconnect()
      host?.removeEventListener('pointermove', handlePointerMove)
      host?.removeEventListener('pointerleave', handlePointerLeave)
      canvas.removeEventListener('webglcontextlost', handleContextLost)
      canvas.removeEventListener('webglcontextrestored', handleContextRestored)
      renderer?.dispose()
    }
  }, [contextGeneration, forcedColors, reducedMotion, reducedTransparency])

  return { canvasRef, layerRef, mode, reducedMotion, reducedTransparency, forcedColors }
}
