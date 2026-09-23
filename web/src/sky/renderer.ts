/**
 * The star field: every visible star drawn as an instanced point sprite in a
 * single draw call.
 *
 * The shape of this is settled by ADR 0003 and confirmed by measurement in
 * ADR 0009 — 60 fps with all 206,636 stars on integrated graphics, with about
 * a five-fold headroom. The prototype it grew from lives in `web/prototype/`
 * and is the instrument for checking that a change has not cost frames.
 */

import { ERAS, ERA_COLOURS, FLARE, FLARE_YEARS, UNDATED_WEIGHT, boundariesUniform, paletteUniform } from './decades'
import { STRIDE } from './tiles'

/** A star as a tile stores it, and as the GPU receives it. */
export interface Star {
  artistId: number
  x: number
  y: number
  brightness: number
  /** The year the act began; 0 when the canon does not know it. */
  year: number
}

/**
 * A star as the interface points at it: which one, and where. What the halo,
 * a flight and the address need, without the drawing data only a tile has.
 */
export type Place = Pick<Star, 'artistId' | 'x' | 'y'>

/**
 * What the observer's instruments ask of the field.
 *
 * Both are shader uniforms rather than a re-upload: moving the time slider
 * changes one number per frame, and rebuilding a buffer of two hundred
 * thousand stars for each step of a drag would be the whole frame budget.
 */
export interface Instruments {
  /** Colour stars by the era their act began in. */
  lens: boolean
  /** Show the sky as it stood in this year; `null` shows all of it. */
  until: number | null
}

/**
 * How the marked star is drawn.
 *
 * The list is data rather than a switch in two places, so the picker and the
 * shader cannot drift apart: the index here *is* the `u_shape` the shader
 * branches on.
 */
export const HALO_SHAPES = ['ripple', 'star', 'sixfold', 'turning', 'both', 'glow'] as const

export type HaloShape = (typeof HALO_SHAPES)[number]

/** What marks the star whose card is open. */
export interface Halo {
  x: number
  y: number
  shape: HaloShape
  /** Linear RGB, each 0..1. */
  colour: [number, number, number]
}

/** Where the camera is looking. */
export interface Camera {
  /** World coordinates at the centre of the viewport. */
  x: number
  y: number
  /** Pixels per world unit. */
  scale: number
}

// The palette and the time curve are written into the shader from
// `decades.ts`, so the legend, the tests and the sky read one set of numbers.
const glsl = (n: number) => (Number.isInteger(n) ? `${String(n)}.0` : String(n))

const VERTEX = `#version 300 es
precision highp float;

// Per instance; the quad itself comes from gl_VertexID, so the buffer holds
// nothing but stars.
in vec2 a_position;
in float a_brightness;
in float a_year;

uniform vec2 u_camera;
uniform float u_scale;
uniform vec2 u_viewport;
uniform float u_time;
uniform float u_twinkle;
// The time machine's year, or 0 when it is off.
uniform float u_until;
// 1 when the era lens is on.
uniform float u_lens;
uniform vec3 u_palette[${String(ERA_COLOURS.length + 1)}];
uniform float u_eras[${String(ERAS.length)}];

out float v_brightness;
out vec2 v_offset;
out vec3 v_colour;
out float v_lensed;

// The same rule as timeWeight() in decades.ts.
float timeWeight(float year, float until) {
  if (year <= 0.0) return ${glsl(UNDATED_WEIGHT)};
  if (year > until) return 0.0;
  return 1.0 + ${glsl(FLARE)} * exp(-(until - year) / ${glsl(FLARE_YEARS)});
}

// The same rule as eraIndex() in decades.ts; the undated colour is last.
vec3 eraColour(float year) {
  if (year <= 0.0) return u_palette[${String(ERA_COLOURS.length)}];
  int index = 0;
  for (int i = 1; i < ${String(ERAS.length)}; i++) {
    if (year >= u_eras[i]) index = i;
  }
  return u_palette[index];
}

void main() {
  float weight = u_until > 0.0 ? timeWeight(a_year, u_until) : 1.0;
  // A star from after the chosen year is not there yet: it is moved outside
  // the clip volume, which costs nothing and draws nothing.
  if (weight <= 0.0) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }

  // Size follows brightness and not zoom: stars are points of light, not
  // discs, and swelling them on approach would read as balloons. A star in
  // its first years under the time machine is drawn larger as well as
  // brighter, which is what makes it read as lighting up.
  float size = mix(2.0, 7.0, a_brightness) * min(weight, 1.6);

  // A per-star phase from its position, so neighbours do not pulse together.
  float phase = a_position.x * 0.7 + a_position.y * 1.3;
  size *= 1.0 - u_twinkle * 0.1 + u_twinkle * 0.1 * sin(u_time * 1.7 + phase);

  vec2 corner = vec2(
    (gl_VertexID == 0 || gl_VertexID == 3 || gl_VertexID == 5) ? -1.0 : 1.0,
    (gl_VertexID < 2 || gl_VertexID == 5) ? -1.0 : 1.0
  );

  vec2 screen = (a_position - u_camera) * u_scale;
  gl_Position = vec4((screen + corner * size) / (u_viewport * 0.5), 0.0, 1.0);

  v_brightness = a_brightness * min(weight, 1.0) + max(weight - 1.0, 0.0) * 0.4;
  v_offset = corner;
  v_colour = eraColour(a_year);
  v_lensed = u_lens;
}`

const FRAGMENT = `#version 300 es
precision highp float;

in float v_brightness;
in vec2 v_offset;
in vec3 v_colour;
in float v_lensed;
out vec4 fragment;

void main() {
  // A gaussian falloff rather than a hard disc: the glow is what makes a
  // field of points read as a sky rather than as a scatter plot.
  float distance = length(v_offset);
  float glow = exp(-4.0 * distance * distance);
  if (glow < 0.01) discard;

  // Faint stars stay the mark's azure, bright ones run warm. Under the lens
  // the colour is the era's instead, and brightness still sets how much of it
  // shows, so the hubs stay the hubs.
  vec3 natural = mix(vec3(0.29, 0.56, 0.91), vec3(1.0, 0.94, 0.85), v_brightness * v_brightness);
  vec3 colour = mix(natural, v_colour, v_lensed);
  float alpha = glow * (0.35 + 0.65 * clamp(v_brightness, 0.0, 1.0));
  fragment = vec4(colour * glow, alpha);
}`

/**
 * A route through the sky: the stops joined in order.
 *
 * Its own tiny program for the same reason the halo has one: the field is a
 * single measured draw call, and a route is a handful of segments.
 */
const ROUTE_VERTEX = `#version 300 es
precision highp float;

in vec2 a_point;
uniform vec2 u_camera;
uniform float u_scale;
uniform vec2 u_viewport;

void main() {
  vec2 screen = (a_point - u_camera) * u_scale;
  gl_Position = vec4(screen / (u_viewport * 0.5), 0.0, 1.0);
}`

const ROUTE_FRAGMENT = `#version 300 es
precision highp float;

uniform vec3 u_colour;
out vec4 fragment;

void main() {
  fragment = vec4(u_colour, 0.55);
}`

/**
 * The halo around the one star being looked at.
 *
 * A second, single-instance pass rather than a flag on every star: the field
 * is one instanced draw call whose cost was measured (ADR 0009), and adding a
 * per-star uniform comparison to it would spend that budget on 206,636 stars
 * to change one. This pass draws one quad.
 *
 * Its size is in **pixels, not world units**, so the halo stays the same size
 * on screen at every zoom — a marker of "this one", not an object in the sky
 * that grows as you approach.
 */
const HALO_VERTEX = `#version 300 es
precision highp float;

uniform vec2 u_position;
uniform vec2 u_camera;
uniform float u_scale;
uniform vec2 u_viewport;

out vec2 v_offset;

void main() {
  vec2 corner = vec2(
    (gl_VertexID == 0 || gl_VertexID == 3 || gl_VertexID == 5) ? -1.0 : 1.0,
    (gl_VertexID < 2 || gl_VertexID == 5) ? -1.0 : 1.0
  );

  vec2 screen = (u_position - u_camera) * u_scale;
  gl_Position = vec4((screen + corner * 26.0) / (u_viewport * 0.5), 0.0, 1.0);
  v_offset = corner;
}`

const HALO_FRAGMENT = `#version 300 es
precision highp float;

in vec2 v_offset;
uniform float u_time;
uniform float u_twinkle;
uniform int u_shape;
uniform vec3 u_colour;
out vec4 fragment;

// A soft core, so the star itself reads as lit rather than covered. Every
// shape keeps it: without one the marked star goes dark at the centre of its
// own marker.
float core(float distance) {
  return exp(-9.0 * distance * distance) * 0.8;
}

// A ring travelling outwards and fading as it goes, which draws the eye even
// where the sky is crowded.
float ripple(float distance, float phase) {
  float radius = mix(0.45, 0.95, phase);
  return exp(-90.0 * (distance - radius) * (distance - radius)) * (1.0 - phase * 0.65);
}

// A steady ring, for a marker that does not move.
float ring(float distance) {
  return exp(-110.0 * (distance - 0.62) * (distance - 0.62)) * 0.7;
}

// Four spikes along the axes: the shape a lens makes of a bright point, and
// the one people draw when asked to draw a star.
//
// The breath argument runs 0..1 and lengthens the spikes as well as
// brightening them. Length is what makes the pulse read as breathing rather
// than as flickering:
// a star that only changes brightness looks like a fault in the display, while
// one that also reaches further looks alive.
float spikes(vec2 offset, float distance, int count, float breath) {
  float angle = atan(offset.y, offset.x);
  // cos(count * angle) peaks once per spike; the power sharpens each peak into
  // a needle rather than a lobe.
  float arms = pow(abs(cos(float(count) * 0.5 * angle)), 24.0);
  // A smaller falloff reaches further, so this is the spikes growing outwards.
  float reach = mix(2.9, 1.7, breath);
  return arms * exp(-reach * distance * distance) * mix(0.62, 0.95, breath);
}

// A slowly turning halo: the same spikes, rotating.
float turning(vec2 offset, float distance, float seconds, float breath) {
  float turn = seconds * 0.35;
  vec2 spun = vec2(offset.x * cos(turn) - offset.y * sin(turn), offset.x * sin(turn) + offset.y * cos(turn));
  return spikes(spun, distance, 4, breath);
}

void main() {
  float distance = length(v_offset);
  if (distance > 1.0) discard;

  // Motion is the pulse; without it every shape holds still.
  float phase = 0.5 + 0.5 * sin(u_time * 2.2);

  // The spikes breathe on their own, much slower clock: about five seconds a
  // cycle against the ripple's three. A marker is meant to be noticed once and
  // then lived with, so it should not keep asking for attention.
  //
  // Smoothed with smoothstep so it lingers at full and at rest rather than
  // sweeping evenly through the middle — a sine alone reads as mechanical.
  float breath = smoothstep(0.0, 1.0, 0.5 + 0.5 * sin(u_time * 1.25));
  // Held at three-quarters when motion is unwanted: the shape at its most
  // legible, simply not moving.
  breath = mix(0.75, breath, u_twinkle);

  float halo = core(distance);
  if (u_shape == 0) {
    halo += mix(ring(distance), ripple(distance, phase), u_twinkle);
  } else if (u_shape == 1) {
    halo += spikes(v_offset, distance, 4, breath);
  } else if (u_shape == 2) {
    halo += spikes(v_offset, distance, 6, breath);
  } else if (u_shape == 3) {
    halo += turning(v_offset, distance, u_time * u_twinkle, breath);
  } else if (u_shape == 4) {
    // Ring and spikes together: the loudest of the set.
    halo += mix(ring(distance), ripple(distance, phase), u_twinkle) * 0.7;
    halo += spikes(v_offset, distance, 4, breath) * 0.7;
  } else {
    // Nothing but the glow: the quietest marker that is still a marker.
    halo += exp(-3.0 * distance * distance) * mix(0.32, 0.5, breath);
  }

  if (halo < 0.01) discard;
  fragment = vec4(u_colour * halo, halo * 0.85);
}`

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type)
  if (!shader) throw new Error('could not create a shader')
  gl.shaderSource(shader, source)
  gl.compileShader(shader)
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(shader)
    gl.deleteShader(shader)
    throw new Error(log ?? 'shader failed to compile')
  }
  return shader
}

/** Draws a star field. One instance per canvas. */
export class SkyRenderer {
  private readonly gl: WebGL2RenderingContext
  private readonly program: WebGLProgram
  private readonly buffer: WebGLBuffer
  private readonly uniforms: Record<
    'camera' | 'scale' | 'viewport' | 'time' | 'twinkle' | 'until' | 'lens' | 'palette' | 'eras',
    WebGLUniformLocation | null
  >
  private readonly route: WebGLProgram
  private readonly routeUniforms: Record<'camera' | 'scale' | 'viewport' | 'colour', WebGLUniformLocation | null>
  private readonly routeVao: WebGLVertexArrayObject | null
  private readonly routeBuffer: WebGLBuffer
  private routePoints = 0
  private readonly halo: WebGLProgram
  private readonly haloUniforms: Record<
    'position' | 'camera' | 'scale' | 'viewport' | 'time' | 'twinkle' | 'shape' | 'colour',
    WebGLUniformLocation | null
  >
  // The field's vertex array carries per-instance attributes; the halo has
  // none, and binding an empty one keeps those divisors out of its draw.
  private readonly emptyVao: WebGLVertexArrayObject | null
  private instances = 0

  constructor(canvas: HTMLCanvasElement) {
    // `alpha: false` lets the compositor skip a blend with the page, and
    // antialiasing buys nothing for round glows.
    const gl = canvas.getContext('webgl2', { alpha: false, antialias: false })
    if (!gl) throw new Error('WebGL2 is not available')
    this.gl = gl

    const program = gl.createProgram()
    if (!program) throw new Error('could not create a program')
    gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERTEX))
    gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAGMENT))
    gl.linkProgram(program)
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(program) ?? 'program failed to link')
    }
    this.program = program
    gl.useProgram(program)

    const buffer = gl.createBuffer()
    if (!buffer) throw new Error('could not create a buffer')
    this.buffer = buffer

    const vao = gl.createVertexArray()
    gl.bindVertexArray(vao)
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer)

    // Interleaved (x, y, brightness, year), which is what `upload` packs.
    const bytes = STRIDE * 4
    const position = gl.getAttribLocation(program, 'a_position')
    const brightness = gl.getAttribLocation(program, 'a_brightness')
    const year = gl.getAttribLocation(program, 'a_year')
    gl.enableVertexAttribArray(position)
    gl.vertexAttribPointer(position, 2, gl.FLOAT, false, bytes, 0)
    gl.vertexAttribDivisor(position, 1)
    gl.enableVertexAttribArray(brightness)
    gl.vertexAttribPointer(brightness, 1, gl.FLOAT, false, bytes, 8)
    gl.vertexAttribDivisor(brightness, 1)
    gl.enableVertexAttribArray(year)
    gl.vertexAttribPointer(year, 1, gl.FLOAT, false, bytes, 12)
    gl.vertexAttribDivisor(year, 1)

    this.uniforms = {
      camera: gl.getUniformLocation(program, 'u_camera'),
      scale: gl.getUniformLocation(program, 'u_scale'),
      viewport: gl.getUniformLocation(program, 'u_viewport'),
      time: gl.getUniformLocation(program, 'u_time'),
      twinkle: gl.getUniformLocation(program, 'u_twinkle'),
      until: gl.getUniformLocation(program, 'u_until'),
      lens: gl.getUniformLocation(program, 'u_lens'),
      palette: gl.getUniformLocation(program, 'u_palette'),
      eras: gl.getUniformLocation(program, 'u_eras'),
    }
    // Constant for the life of the page, so set once rather than per frame.
    gl.uniform3fv(this.uniforms.palette, paletteUniform())
    gl.uniform1fv(this.uniforms.eras, boundariesUniform())

    const halo = gl.createProgram()
    if (!halo) throw new Error('could not create a program')
    gl.attachShader(halo, compile(gl, gl.VERTEX_SHADER, HALO_VERTEX))
    gl.attachShader(halo, compile(gl, gl.FRAGMENT_SHADER, HALO_FRAGMENT))
    gl.linkProgram(halo)
    if (!gl.getProgramParameter(halo, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(halo) ?? 'the halo program failed to link')
    }
    this.halo = halo
    this.haloUniforms = {
      position: gl.getUniformLocation(halo, 'u_position'),
      camera: gl.getUniformLocation(halo, 'u_camera'),
      scale: gl.getUniformLocation(halo, 'u_scale'),
      viewport: gl.getUniformLocation(halo, 'u_viewport'),
      time: gl.getUniformLocation(halo, 'u_time'),
      twinkle: gl.getUniformLocation(halo, 'u_twinkle'),
      shape: gl.getUniformLocation(halo, 'u_shape'),
      colour: gl.getUniformLocation(halo, 'u_colour'),
    }
    this.emptyVao = gl.createVertexArray()

    const route = gl.createProgram()
    if (!route) throw new Error('could not create a program')
    gl.attachShader(route, compile(gl, gl.VERTEX_SHADER, ROUTE_VERTEX))
    gl.attachShader(route, compile(gl, gl.FRAGMENT_SHADER, ROUTE_FRAGMENT))
    gl.linkProgram(route)
    if (!gl.getProgramParameter(route, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(route) ?? 'the route program failed to link')
    }
    this.route = route
    this.routeUniforms = {
      camera: gl.getUniformLocation(route, 'u_camera'),
      scale: gl.getUniformLocation(route, 'u_scale'),
      viewport: gl.getUniformLocation(route, 'u_viewport'),
      colour: gl.getUniformLocation(route, 'u_colour'),
    }
    const routeBuffer = gl.createBuffer()
    if (!routeBuffer) throw new Error('could not create a buffer')
    this.routeBuffer = routeBuffer
    this.routeVao = gl.createVertexArray()
    gl.bindVertexArray(this.routeVao)
    gl.bindBuffer(gl.ARRAY_BUFFER, routeBuffer)
    const point = gl.getAttribLocation(route, 'a_point')
    gl.enableVertexAttribArray(point)
    gl.vertexAttribPointer(point, 2, gl.FLOAT, false, 8, 0)

    gl.bindVertexArray(vao)

    // Additive blending, because light adds: overlapping stars brighten
    // rather than occlude. Nothing here needs a depth buffer.
    gl.enable(gl.BLEND)
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE)
  }

  /** Replaces the field. `packed` is (x, y, brightness, year) quadruples. */
  upload(packed: Float32Array): void {
    this.gl.bindBuffer(this.gl.ARRAY_BUFFER, this.buffer)
    this.gl.bufferData(this.gl.ARRAY_BUFFER, packed, this.gl.STATIC_DRAW)
    this.instances = packed.length / STRIDE
  }

  /** Replaces the route: its stops in order, as world coordinates. */
  setRoute(points: readonly { x: number; y: number }[]): void {
    const gl = this.gl
    const data = new Float32Array(points.flatMap(point => [point.x, point.y]))
    gl.bindBuffer(gl.ARRAY_BUFFER, this.routeBuffer)
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW)
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer)
    this.routePoints = points.length
  }

  resize(width: number, height: number): void {
    this.gl.viewport(0, 0, width, height)
  }

  /**
   * Draws one frame.
   *
   * `twinkle` is 0 or 1 rather than a boolean so the caller can honour
   * `prefers-reduced-motion` without a branch in the shader.
   */
  draw(
    camera: Camera,
    viewport: [number, number],
    seconds: number,
    twinkle: number,
    marked?: Halo | null,
    instruments: Instruments = { lens: false, until: null }
  ): void {
    const gl = this.gl
    gl.clearColor(0.027, 0.031, 0.051, 1)
    gl.clear(gl.COLOR_BUFFER_BIT)
    if (this.instances === 0) return

    gl.useProgram(this.program)
    gl.uniform2f(this.uniforms.camera, camera.x, camera.y)
    gl.uniform1f(this.uniforms.scale, camera.scale)
    gl.uniform2f(this.uniforms.viewport, viewport[0], viewport[1])
    gl.uniform1f(this.uniforms.time, seconds)
    gl.uniform1f(this.uniforms.twinkle, twinkle)
    gl.uniform1f(this.uniforms.until, instruments.until ?? 0)
    gl.uniform1f(this.uniforms.lens, instruments.lens ? 1 : 0)

    // The whole sky, one call.
    gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, this.instances)

    // The route over the field and under the halo: a line is context, the
    // halo is the one star being looked at.
    if (this.routePoints > 1) {
      const vao = gl.getParameter(gl.VERTEX_ARRAY_BINDING) as WebGLVertexArrayObject | null
      gl.bindVertexArray(this.routeVao)
      gl.useProgram(this.route)
      gl.uniform2f(this.routeUniforms.camera, camera.x, camera.y)
      gl.uniform1f(this.routeUniforms.scale, camera.scale)
      gl.uniform2f(this.routeUniforms.viewport, viewport[0], viewport[1])
      gl.uniform3f(this.routeUniforms.colour, 0.55, 0.75, 1.0)
      gl.drawArrays(gl.LINE_STRIP, 0, this.routePoints)
      gl.bindVertexArray(vao)
    }

    // The marked star last, so its halo sits over its neighbours rather than
    // under them. One quad; the field's cost is untouched.
    if (marked) {
      const vao = gl.getParameter(gl.VERTEX_ARRAY_BINDING) as WebGLVertexArrayObject | null
      gl.bindVertexArray(this.emptyVao)
      gl.useProgram(this.halo)
      gl.uniform2f(this.haloUniforms.position, marked.x, marked.y)
      gl.uniform2f(this.haloUniforms.camera, camera.x, camera.y)
      gl.uniform1f(this.haloUniforms.scale, camera.scale)
      gl.uniform2f(this.haloUniforms.viewport, viewport[0], viewport[1])
      gl.uniform1f(this.haloUniforms.time, seconds)
      gl.uniform1f(this.haloUniforms.twinkle, twinkle)
      gl.uniform1i(this.haloUniforms.shape, HALO_SHAPES.indexOf(marked.shape))
      gl.uniform3f(this.haloUniforms.colour, ...marked.colour)
      gl.drawArrays(gl.TRIANGLES, 0, 6)
      gl.bindVertexArray(vao)
    }
  }

  /** How many stars the last upload put on the GPU. */
  get starCount(): number {
    return this.instances
  }
}
