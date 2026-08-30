/**
 * The star field: every visible star drawn as an instanced point sprite in a
 * single draw call.
 *
 * The shape of this is settled by ADR 0003 and confirmed by measurement in
 * ADR 0009 — 60 fps with all 206,636 stars on integrated graphics, with about
 * a five-fold headroom. The prototype it grew from lives in `web/prototype/`
 * and is the instrument for checking that a change has not cost frames.
 */

/** A star as a tile stores it, and as the GPU receives it. */
export interface Star {
  artistId: number
  x: number
  y: number
  brightness: number
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

const VERTEX = `#version 300 es
precision highp float;

// Per instance; the quad itself comes from gl_VertexID, so the buffer holds
// nothing but stars.
in vec2 a_position;
in float a_brightness;

uniform vec2 u_camera;
uniform float u_scale;
uniform vec2 u_viewport;
uniform float u_time;
uniform float u_twinkle;

out float v_brightness;
out vec2 v_offset;

void main() {
  // Size follows brightness and not zoom: stars are points of light, not
  // discs, and swelling them on approach would read as balloons.
  float size = mix(2.0, 7.0, a_brightness);

  // A per-star phase from its position, so neighbours do not pulse together.
  float phase = a_position.x * 0.7 + a_position.y * 1.3;
  size *= 1.0 - u_twinkle * 0.1 + u_twinkle * 0.1 * sin(u_time * 1.7 + phase);

  vec2 corner = vec2(
    (gl_VertexID == 0 || gl_VertexID == 3 || gl_VertexID == 5) ? -1.0 : 1.0,
    (gl_VertexID < 2 || gl_VertexID == 5) ? -1.0 : 1.0
  );

  vec2 screen = (a_position - u_camera) * u_scale;
  gl_Position = vec4((screen + corner * size) / (u_viewport * 0.5), 0.0, 1.0);

  v_brightness = a_brightness;
  v_offset = corner;
}`

const FRAGMENT = `#version 300 es
precision highp float;

in float v_brightness;
in vec2 v_offset;
out vec4 fragment;

void main() {
  // A gaussian falloff rather than a hard disc: the glow is what makes a
  // field of points read as a sky rather than as a scatter plot.
  float distance = length(v_offset);
  float glow = exp(-4.0 * distance * distance);
  if (glow < 0.01) discard;

  // Faint stars stay the mark's azure, bright ones run warm.
  vec3 colour = mix(vec3(0.29, 0.56, 0.91), vec3(1.0, 0.94, 0.85), v_brightness * v_brightness);
  fragment = vec4(colour * glow, glow * (0.35 + 0.65 * v_brightness));
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
  private readonly uniforms: Record<'camera' | 'scale' | 'viewport' | 'time' | 'twinkle', WebGLUniformLocation | null>
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

    // Interleaved (x, y, brightness), which is what `upload` packs.
    const position = gl.getAttribLocation(program, 'a_position')
    const brightness = gl.getAttribLocation(program, 'a_brightness')
    gl.enableVertexAttribArray(position)
    gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 12, 0)
    gl.vertexAttribDivisor(position, 1)
    gl.enableVertexAttribArray(brightness)
    gl.vertexAttribPointer(brightness, 1, gl.FLOAT, false, 12, 8)
    gl.vertexAttribDivisor(brightness, 1)

    this.uniforms = {
      camera: gl.getUniformLocation(program, 'u_camera'),
      scale: gl.getUniformLocation(program, 'u_scale'),
      viewport: gl.getUniformLocation(program, 'u_viewport'),
      time: gl.getUniformLocation(program, 'u_time'),
      twinkle: gl.getUniformLocation(program, 'u_twinkle'),
    }

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
    gl.bindVertexArray(vao)

    // Additive blending, because light adds: overlapping stars brighten
    // rather than occlude. Nothing here needs a depth buffer.
    gl.enable(gl.BLEND)
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE)
  }

  /** Replaces the field. `packed` is (x, y, brightness) triples. */
  upload(packed: Float32Array): void {
    this.gl.bindBuffer(this.gl.ARRAY_BUFFER, this.buffer)
    this.gl.bufferData(this.gl.ARRAY_BUFFER, packed, this.gl.STATIC_DRAW)
    this.instances = packed.length / 3
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
  draw(camera: Camera, viewport: [number, number], seconds: number, twinkle: number, marked?: Halo | null): void {
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

    // The whole sky, one call.
    gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, this.instances)

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
