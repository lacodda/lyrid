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
  draw(camera: Camera, viewport: [number, number], seconds: number, twinkle: number): void {
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
  }

  /** How many stars the last upload put on the GPU. */
  get starCount(): number {
    return this.instances
  }
}
