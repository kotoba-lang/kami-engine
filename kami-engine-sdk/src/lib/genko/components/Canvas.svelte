<script lang="ts">
  /**
   * WebGPU Canvas wrapper for Genko manga editor.
   * Manages GPU init, render loop, pointer/stylus input, and image overlays.
   * Canvas rendering logic stays as imperative JS (WebGPU doesn't benefit from Svelte reactivity).
   */
  import { onMount, onDestroy } from 'svelte';
  import { getStrokes, getOverlays, getSelectedIdx, setSelectedIdx, consumeRedraw, requestRedraw, findByNid } from '../stores/doc.svelte';
  import { runCanvasOp, type CanvasCtx, type CanvasOp } from '../canvas-pregel';

  let { width = 0, height = 0, zoom = 1, panX = 0, panY = 0, activeYoushi = 'b4manga',
    activeBrush = 'fine', activeMode = 'draw', brushColor = [0.2,0.2,0.2,1] as number[], brushSize = 2, brushOpacity = 1,
    faceAddMode = false,
    onzoomchange, onpanchange, onstrokeend, onoverlayadd, onselect, onmove, ondragend, onfaceadd,
  }: {
    width?: number; height?: number;
    zoom?: number; panX?: number; panY?: number;
    activeYoushi?: string;
    activeBrush?: string; activeMode?: string;
    brushColor?: number[]; brushSize?: number; brushOpacity?: number;
    faceAddMode?: boolean;
    onzoomchange?: (z: number) => void;
    onpanchange?: (x: number, y: number) => void;
    onstrokeend?: (stroke: Record<string, unknown>) => void;
    onoverlayadd?: (overlay: Record<string, unknown>) => void;
    onselect?: (idx: number) => void;
    onmove?: () => void;
    ondragend?: (info: { nid: string; type: string; before: any; after: any }) => void;
    onfaceadd?: (imageNid: string, cx: number, cy: number) => void;
    /** Provided by Genko — runs a canvas op through the Pregel pipeline. */
    runop?: (op: CanvasOp) => void;
  } = $props();

  let canvasEl: HTMLCanvasElement;
  let imgLayer: HTMLDivElement;
  let textLayer: HTMLDivElement;
  let fukidashiLayer: SVGSVGElement;
  let handleLayer: HTMLDivElement;
  let device: GPUDevice | null = null;
  let ctx: GPUCanvasContext | null = null;
  let pipeline: GPURenderPipeline | null = null;
  let vpUniformBuf: GPUBuffer | null = null;
  let vpBindGroup: GPUBindGroup | null = null;
  let vertBuf: GPUBuffer | null = null;
  let gl: WebGLRenderingContext | WebGL2RenderingContext | null = null;
  let glProgram: WebGLProgram | null = null;
  let glVertBuf: WebGLBuffer | null = null;
  let glPosLoc = -1;
  let glColorLoc = -1;
  let glZoomLoc: WebGLUniformLocation | null = null;
  let glPanLoc: WebGLUniformLocation | null = null;
  let glCanvasLoc: WebGLUniformLocation | null = null;
  let animId = 0;
  let dpr = 1;
  let gpuError = $state('');
  let renderBackend = $state<'webgpu' | 'webgl' | 'none'>('none');
  let renderFrame: ((data: Float32Array, vertCount: number) => void) | null = null;
  const MAX_VERTS = 2_000_000;

  // Drawing state
  let isDrawing = false;
  let currentStroke: Record<string, unknown> | null = null;
  let isPanning = false;
  let panStartX = 0, panStartY = 0, panStartPX = 0, panStartPY = 0;

  // Youshi templates (genkouyoushi). Frame coords match Shueisha/Jump B4 manga template.
  type YoushiDef = {
    wMM: number; hMM: number; draw: boolean;
    trimL?: number; trimT?: number; trimR?: number; trimB?: number;
    outerL?: number; outerT?: number; outerR?: number; outerB?: number;
    innerL?: number; innerT?: number; innerR?: number; innerB?: number;
    rulerStep?: number;
  };
  const YOUSHI: Record<string, YoushiDef> = {
    b4manga: { wMM: 257, hMM: 364, draw: true,
      trimL: 18, trimT: 18, trimR: 239, trimB: 346,
      outerL: 25, outerT: 27, outerR: 232, outerB: 337,
      innerL: 53.5, innerT: 72, innerR: 203.5, innerB: 292,
      rulerStep: 5 },
    b4koma: { wMM: 257, hMM: 364, draw: true,
      trimL: 18, trimT: 18, trimR: 239, trimB: 346,
      outerL: 25, outerT: 27, outerR: 232, outerB: 337,
      innerL: 53.5, innerT: 72, innerR: 203.5, innerB: 292,
      rulerStep: 5 },
    none: { wMM: 210, hMM: 297, draw: false },
  };

  const SHADER = `
struct VP { zoom:f32, panX:f32, panY:f32, cw:f32, ch:f32, _p0:f32, _p1:f32, _p2:f32 };
@group(0) @binding(0) var<uniform> vp:VP;
struct VIn { @location(0) pos:vec2f, @location(1) col:vec4f };
struct VOut { @builtin(position) pos:vec4f, @location(0) col:vec4f };
@vertex fn vs(v:VIn)->VOut {
  var o:VOut;
  let x=(v.pos.x*vp.cw*vp.zoom+vp.panX)/vp.cw*2.0-1.0;
  let y=1.0-(v.pos.y*vp.ch*vp.zoom+vp.panY)/vp.ch*2.0;
  o.pos=vec4f(x,y,0,1); o.col=v.col; return o;
}
@fragment fn fs(v:VOut)->@location(0) vec4f { return v.col; }`;

  const WEBGL_VERTEX_SHADER = `
attribute vec2 a_pos;
attribute vec4 a_color;
uniform float u_zoom;
uniform vec2 u_pan;
uniform vec2 u_canvas;
varying vec4 v_color;

void main() {
  float x = ((a_pos.x * u_canvas.x * u_zoom) + u_pan.x) / u_canvas.x * 2.0 - 1.0;
  float y = 1.0 - (((a_pos.y * u_canvas.y * u_zoom) + u_pan.y) / u_canvas.y * 2.0);
  gl_Position = vec4(x, y, 0.0, 1.0);
  v_color = a_color;
}
`;

  const WEBGL_FRAGMENT_SHADER = `
precision mediump float;
varying vec4 v_color;

void main() {
  gl_FragColor = v_color;
}
`;

  async function initGPU() {
    if (!navigator.gpu) throw new Error('WebGPU is not available in this browser');
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error('No GPU adapter');
    device = await adapter.requestDevice();
    ctx = canvasEl.getContext('webgpu') as GPUCanvasContext | null;
    if (!ctx) throw new Error('Failed to acquire WebGPU canvas context');
    const fmt = navigator.gpu.getPreferredCanvasFormat();
    ctx.configure({ device, format: fmt, alphaMode: 'premultiplied' });

    const mod = device.createShaderModule({ code: SHADER });
    pipeline = device.createRenderPipeline({
      layout: 'auto',
      vertex: { module: mod, entryPoint: 'vs', buffers: [{ arrayStride: 24, attributes: [{ shaderLocation: 0, offset: 0, format: 'float32x2' }, { shaderLocation: 1, offset: 8, format: 'float32x4' }] }] },
      fragment: { module: mod, entryPoint: 'fs', targets: [{ format: fmt, blend: { color: { srcFactor: 'src-alpha', dstFactor: 'one-minus-src-alpha' }, alpha: { srcFactor: 'one', dstFactor: 'one-minus-src-alpha' } } }] },
      primitive: { topology: 'triangle-list' },
    });

    vpUniformBuf = device.createBuffer({ size: 32, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    const bgl = pipeline.getBindGroupLayout(0);
    vpBindGroup = device.createBindGroup({ layout: bgl, entries: [{ binding: 0, resource: { buffer: vpUniformBuf } }] });
    vertBuf = device.createBuffer({ size: MAX_VERTS * 24, usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST });
    renderBackend = 'webgpu';
    renderFrame = renderWithWebGPU;
  }

  function compileWebGLShader(
    glCtx: WebGLRenderingContext | WebGL2RenderingContext,
    type: number,
    source: string,
  ): WebGLShader {
    const shader = glCtx.createShader(type);
    if (!shader) throw new Error('Failed to create WebGL shader');
    glCtx.shaderSource(shader, source);
    glCtx.compileShader(shader);
    if (!glCtx.getShaderParameter(shader, glCtx.COMPILE_STATUS)) {
      const log = glCtx.getShaderInfoLog(shader) || 'Unknown WebGL shader compile error';
      glCtx.deleteShader(shader);
      throw new Error(log);
    }
    return shader;
  }

  function initWebGL() {
    const webgl =
      canvasEl.getContext('webgl2', { alpha: true, antialias: true }) ||
      canvasEl.getContext('webgl', { alpha: true, antialias: true });
    if (!webgl) throw new Error('WebGL is not available in this browser');

    const vertexShader = compileWebGLShader(webgl, webgl.VERTEX_SHADER, WEBGL_VERTEX_SHADER);
    const fragmentShader = compileWebGLShader(webgl, webgl.FRAGMENT_SHADER, WEBGL_FRAGMENT_SHADER);
    const program = webgl.createProgram();
    if (!program) throw new Error('Failed to create WebGL program');
    webgl.attachShader(program, vertexShader);
    webgl.attachShader(program, fragmentShader);
    webgl.linkProgram(program);
    if (!webgl.getProgramParameter(program, webgl.LINK_STATUS)) {
      const log = webgl.getProgramInfoLog(program) || 'Unknown WebGL link error';
      throw new Error(log);
    }

    const buffer = webgl.createBuffer();
    if (!buffer) throw new Error('Failed to create WebGL vertex buffer');

    gl = webgl;
    glProgram = program;
    glVertBuf = buffer;
    glPosLoc = webgl.getAttribLocation(program, 'a_pos');
    glColorLoc = webgl.getAttribLocation(program, 'a_color');
    glZoomLoc = webgl.getUniformLocation(program, 'u_zoom');
    glPanLoc = webgl.getUniformLocation(program, 'u_pan');
    glCanvasLoc = webgl.getUniformLocation(program, 'u_canvas');

    webgl.bindBuffer(webgl.ARRAY_BUFFER, buffer);
    webgl.enable(webgl.BLEND);
    webgl.blendFunc(webgl.SRC_ALPHA, webgl.ONE_MINUS_SRC_ALPHA);
    renderBackend = 'webgl';
    renderFrame = renderWithWebGL;
  }

  function renderWithWebGPU(data: Float32Array, vertCount: number) {
    if (!device || !ctx || !pipeline || !vpUniformBuf || !vpBindGroup || !vertBuf) return;
    device.queue.writeBuffer(vpUniformBuf, 0, new Float32Array([zoom, panX, panY, canvasEl.width, canvasEl.height, 0, 0, 0]));
    if (vertCount > 0 && vertCount <= MAX_VERTS) {
      const uploadData = new Float32Array(data.length);
      uploadData.set(data);
      device.queue.writeBuffer(vertBuf, 0, uploadData);
    }
    const enc = device.createCommandEncoder();
    const pass = enc.beginRenderPass({
      colorAttachments: [{
        view: ctx.getCurrentTexture().createView(),
        loadOp: 'clear',
        clearValue: { r: 0.7, g: 0.7, b: 0.7, a: 1 },
        storeOp: 'store',
      }],
    });
    if (vertCount > 0) {
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, vpBindGroup);
      pass.setVertexBuffer(0, vertBuf);
      pass.draw(vertCount);
    }
    pass.end();
    device.queue.submit([enc.finish()]);
  }

  function renderWithWebGL(data: Float32Array, vertCount: number) {
    if (!gl || !glProgram || !glVertBuf || !glZoomLoc || !glPanLoc || !glCanvasLoc) return;
    gl.viewport(0, 0, canvasEl.width, canvasEl.height);
    gl.clearColor(0.7, 0.7, 0.7, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(glProgram);
    gl.uniform1f(glZoomLoc, zoom);
    gl.uniform2f(glPanLoc, panX, panY);
    gl.uniform2f(glCanvasLoc, canvasEl.width, canvasEl.height);
    gl.bindBuffer(gl.ARRAY_BUFFER, glVertBuf);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(glPosLoc);
    gl.vertexAttribPointer(glPosLoc, 2, gl.FLOAT, false, 24, 0);
    gl.enableVertexAttribArray(glColorLoc);
    gl.vertexAttribPointer(glColorLoc, 4, gl.FLOAT, false, 24, 8);
    if (vertCount > 0) gl.drawArrays(gl.TRIANGLES, 0, vertCount);
  }

  function tessellateAll(): Float32Array {
    const verts: number[] = [];
    const cw = canvasEl.width, ch = canvasEl.height;
    const strokes = getStrokes();
    const overlays = getOverlays();

    // Youshi template + paper frame computation
    // Fixed paper size (canvas-size-independent): 2.4 CSS px / mm ≈ 617×874 CSS px for B4.
    // sc is in device pixels / mm (because tessellator outputs `mm * sc / cw` against device cw).
    const FIXED_PX_PER_MM_CSS = 2.4;
    const y = YOUSHI[activeYoushi];
    let paperSc = 0, paperOx = 0, paperOy = 0;
    if (y?.draw) {
      const sc = FIXED_PX_PER_MM_CSS * dpr;
      const pw = y.wMM * sc / cw, ph = y.hMM * sc / ch;
      const ox = (1 - pw) / 2, oy = (1 - ph) / 2;
      paperSc = sc; paperOx = ox; paperOy = oy;
      // Paper background (light gray; almost white)
      const c = [0.98, 0.98, 0.97, 1];
      verts.push(ox, oy, c[0], c[1], c[2], c[3], ox + pw, oy, c[0], c[1], c[2], c[3], ox + pw, oy + ph, c[0], c[1], c[2], c[3]);
      verts.push(ox, oy, c[0], c[1], c[2], c[3], ox + pw, oy + ph, c[0], c[1], c[2], c[3], ox, oy + ph, c[0], c[1], c[2], c[3]);

      // === Genkouyoushi frame lines (罫線/枠/トンボ) — mm coords, paper-relative ===
      // Convert mm to normalized canvas coords: nx = ox + mm*sc/cw, ny = oy + mm*sc/ch
      const mmX = (m: number) => ox + m * sc / cw;
      const mmY = (m: number) => oy + m * sc / ch;
      const drawLine = (x1m: number, y1m: number, x2m: number, y2m: number, r: number, g: number, b: number, a: number, wmm: number) => {
        const px1 = x1m * sc, py1 = y1m * sc, px2 = x2m * sc, py2 = y2m * sc;
        const dx = px2 - px1, dy = py2 - py1, len = Math.sqrt(dx * dx + dy * dy) || 1;
        const hw = Math.max(wmm * sc * 0.5, 0.5);  // minimum 0.5 device px so lines stay visible at any sc
        const nxv = -dy / len * hw, nyv = dx / len * hw;
        const xA = mmX(x1m), yA = mmY(y1m), xB = mmX(x2m), yB = mmY(y2m);
        const nxN = nxv / cw, nyN = nyv / ch;
        verts.push(xA + nxN, yA + nyN, r, g, b, a, xA - nxN, yA - nyN, r, g, b, a, xB + nxN, yB + nyN, r, g, b, a);
        verts.push(xA - nxN, yA - nyN, r, g, b, a, xB - nxN, yB - nyN, r, g, b, a, xB + nxN, yB + nyN, r, g, b, a);
      };
      const drawRect = (x1m: number, y1m: number, x2m: number, y2m: number, r: number, g: number, b: number, a: number) => {
        const u1 = mmX(x1m), v1 = mmY(y1m), u2 = mmX(x2m), v2 = mmY(y2m);
        verts.push(u1, v1, r, g, b, a, u2, v1, r, g, b, a, u2, v2, r, g, b, a);
        verts.push(u1, v1, r, g, b, a, u2, v2, r, g, b, a, u1, v2, r, g, b, a);
      };
      // Ruler margin bands (淡い水色, 15mm wide)
      const rm = 15;
      drawRect(0, 0, y.wMM, rm, 0.88, 0.94, 0.97, 1);
      drawRect(0, y.hMM - rm, y.wMM, y.hMM, 0.88, 0.94, 0.97, 1);
      drawRect(0, rm, rm, y.hMM - rm, 0.88, 0.94, 0.97, 1);
      drawRect(y.wMM - rm, rm, y.wMM, y.hMM - rm, 0.88, 0.94, 0.97, 1);
      // Ruler ticks (5mm spacing)
      const CB: [number, number, number] = [0.55, 0.78, 0.92];
      const step = y.rulerStep || 5;
      for (let m = 0; m <= y.wMM; m += step) {
        drawLine(m, 0, m, 4, CB[0], CB[1], CB[2], 0.7, 0.3);
        drawLine(m, y.hMM, m, y.hMM - 4, CB[0], CB[1], CB[2], 0.7, 0.3);
      }
      for (let m = 0; m <= y.hMM; m += step) {
        drawLine(0, m, 4, m, CB[0], CB[1], CB[2], 0.7, 0.3);
        drawLine(y.wMM, m, y.wMM - 4, m, CB[0], CB[1], CB[2], 0.7, 0.3);
      }
      // 裁ち落とし枠 (trim frame, thin)
      if (y.trimL != null) {
        const tl = y.trimL!, tt = y.trimT!, tr = y.trimR!, tb = y.trimB!;
        drawLine(tl, tt, tr, tt, CB[0], CB[1], CB[2], 0.6, 0.3);
        drawLine(tr, tt, tr, tb, CB[0], CB[1], CB[2], 0.6, 0.3);
        drawLine(tr, tb, tl, tb, CB[0], CB[1], CB[2], 0.6, 0.3);
        drawLine(tl, tb, tl, tt, CB[0], CB[1], CB[2], 0.6, 0.3);
        // トンボ (corner trim marks)
        const tmLen = 10;
        const corners: Array<[number, number, number, number]> = [[tl, tt, -1, -1], [tr, tt, 1, -1], [tr, tb, 1, 1], [tl, tb, -1, 1]];
        for (const [cx, cy, sxd, syd] of corners) {
          drawLine(cx, cy, cx - sxd * tmLen, cy, 0, 0, 0, 0.5, 0.25);
          drawLine(cx, cy, cx, cy - syd * tmLen, 0, 0, 0, 0.5, 0.25);
        }
      }
      // 基本枠 (outer frame, medium)
      if (y.outerL != null) {
        const ol = y.outerL!, ot = y.outerT!, oRr = y.outerR!, ob = y.outerB!;
        drawLine(ol, ot, oRr, ot, CB[0], CB[1], CB[2], 0.8, 0.5);
        drawLine(oRr, ot, oRr, ob, CB[0], CB[1], CB[2], 0.8, 0.5);
        drawLine(oRr, ob, ol, ob, CB[0], CB[1], CB[2], 0.8, 0.5);
        drawLine(ol, ob, ol, ot, CB[0], CB[1], CB[2], 0.8, 0.5);
      }
      // 内枠 (inner safe frame, thick)
      if (y.innerL != null) {
        const il = y.innerL!, it = y.innerT!, ir = y.innerR!, ib = y.innerB!;
        drawLine(il, it, ir, it, CB[0], CB[1], CB[2], 0.9, 0.7);
        drawLine(ir, it, ir, ib, CB[0], CB[1], CB[2], 0.9, 0.7);
        drawLine(ir, ib, il, ib, CB[0], CB[1], CB[2], 0.9, 0.7);
        drawLine(il, ib, il, it, CB[0], CB[1], CB[2], 0.9, 0.7);
      }
    }
    // Helper: map node coord to normalized canvas (mm-aware for _unit==='mm' nodes).
    const nodeX = (o: any, key: string): number => {
      const v = o[key] as number;
      if (paperSc > 0 && o._unit === 'mm') return paperOx + v * paperSc / cw;
      return v / cw;
    };
    const nodeY = (o: any, key: string): number => {
      const v = o[key] as number;
      if (paperSc > 0 && o._unit === 'mm') return paperOy + v * paperSc / ch;
      return v / ch;
    };

    // Strokes
    for (const s of strokes) {
      if ((s._visible as boolean) === false) continue;
      const pts = s.points as Array<{ x: number; y: number; pressure: number }>;
      if (!pts || pts.length < 2) continue;
      const c = (s.color as number[]) || [0, 0, 0, 1];
      const sz = (s.size as number) || 2;
      for (let i = 0; i < pts.length - 1; i++) {
        const a = pts[i], b = pts[i + 1];
        const dx = b.x - a.x, dy = b.y - a.y;
        const len = Math.sqrt(dx * dx + dy * dy) || 1;
        const nx = -dy / len, ny = dx / len;
        const ra = sz * a.pressure * dpr * 0.5, rb = sz * b.pressure * dpr * 0.5;
        verts.push((a.x + nx * ra) / cw, (a.y + ny * ra) / ch, c[0], c[1], c[2], 1);
        verts.push((a.x - nx * ra) / cw, (a.y - ny * ra) / ch, c[0], c[1], c[2], 1);
        verts.push((b.x + nx * rb) / cw, (b.y + ny * rb) / ch, c[0], c[1], c[2], 1);
        verts.push((a.x - nx * ra) / cw, (a.y - ny * ra) / ch, c[0], c[1], c[2], 1);
        verts.push((b.x - nx * rb) / cw, (b.y - ny * rb) / ch, c[0], c[1], c[2], 1);
        verts.push((b.x + nx * rb) / cw, (b.y + ny * rb) / ch, c[0], c[1], c[2], 1);
      }
    }

    // Panel overlays (border rectangles)
    for (const o of overlays) {
      if ((o._visible as boolean) === false) continue;
      if (o.type === 'panel' || o.type === 'tone') {
        const x1 = nodeX(o, 'x1'), y1 = nodeY(o, 'y1');
        const x2 = nodeX(o, 'x2'), y2 = nodeY(o, 'y2');
        const bw = 2 / cw;
        const c = o.type === 'panel' ? [0.2, 0.2, 0.2, 0.8] : [0.5, 0.5, 0.5, 0.3];
        // Top border
        verts.push(x1, y1, c[0], c[1], c[2], c[3], x2, y1, c[0], c[1], c[2], c[3], x2, y1 + bw, c[0], c[1], c[2], c[3]);
        verts.push(x1, y1, c[0], c[1], c[2], c[3], x2, y1 + bw, c[0], c[1], c[2], c[3], x1, y1 + bw, c[0], c[1], c[2], c[3]);
        // Bottom border
        verts.push(x1, y2 - bw, c[0], c[1], c[2], c[3], x2, y2 - bw, c[0], c[1], c[2], c[3], x2, y2, c[0], c[1], c[2], c[3]);
        verts.push(x1, y2 - bw, c[0], c[1], c[2], c[3], x2, y2, c[0], c[1], c[2], c[3], x1, y2, c[0], c[1], c[2], c[3]);
        // Left border
        verts.push(x1, y1, c[0], c[1], c[2], c[3], x1 + bw, y1, c[0], c[1], c[2], c[3], x1 + bw, y2, c[0], c[1], c[2], c[3]);
        verts.push(x1, y1, c[0], c[1], c[2], c[3], x1 + bw, y2, c[0], c[1], c[2], c[3], x1, y2, c[0], c[1], c[2], c[3]);
        // Right border
        verts.push(x2 - bw, y1, c[0], c[1], c[2], c[3], x2, y1, c[0], c[1], c[2], c[3], x2, y2, c[0], c[1], c[2], c[3]);
        verts.push(x2 - bw, y1, c[0], c[1], c[2], c[3], x2, y2, c[0], c[1], c[2], c[3], x2 - bw, y2, c[0], c[1], c[2], c[3]);
      }
    }

    // Selection highlight
    const selIdx = getSelectedIdx();
    if (selIdx >= 0) {
      const strokes = getStrokes();
      const overlays = getOverlays();
      if (selIdx >= strokes.length) {
        const o = overlays[selIdx - strokes.length];
        if (o && o.x1 != null) {
          const x1 = nodeX(o, 'x1'), y1 = nodeY(o, 'y1');
          const x2 = nodeX(o, 'x2'), y2 = nodeY(o, 'y2');
          const bw = 3 / cw;
          const c = [0.88, 0.25, 0.56, 0.9];
          verts.push(x1 - bw, y1 - bw, c[0], c[1], c[2], c[3], x2 + bw, y1 - bw, c[0], c[1], c[2], c[3], x2 + bw, y1 + bw, c[0], c[1], c[2], c[3]);
          verts.push(x1 - bw, y1 - bw, c[0], c[1], c[2], c[3], x2 + bw, y1 + bw, c[0], c[1], c[2], c[3], x1 - bw, y1 + bw, c[0], c[1], c[2], c[3]);
          verts.push(x1 - bw, y2 - bw, c[0], c[1], c[2], c[3], x2 + bw, y2 - bw, c[0], c[1], c[2], c[3], x2 + bw, y2 + bw, c[0], c[1], c[2], c[3]);
          verts.push(x1 - bw, y2 - bw, c[0], c[1], c[2], c[3], x2 + bw, y2 + bw, c[0], c[1], c[2], c[3], x1 - bw, y2 + bw, c[0], c[1], c[2], c[3]);
        }
      }
    }

    return new Float32Array(verts);
  }

  // Convert overlay coord to canvas-internal pixels (mm-aware via _unit==='mm').
  // Paper size is fixed in mm; zoom is applied downstream by renderImages /
  // GPU shader / tessellator. Keep sc=2.4*dpr so coordinates stay paper-relative.
  function paperFrame() {
    const cw = canvasEl.width, ch = canvasEl.height;
    const y = YOUSHI[activeYoushi];
    if (!y?.draw) return { sc: 0, ox: 0, oy: 0, cw, ch };
    const sc = 2.4 * dpr;
    const pw = y.wMM * sc, ph = y.hMM * sc;
    return { sc, ox: (cw - pw) / 2, oy: (ch - ph) / 2, cw, ch };
  }
  function ovX(o: any, key: string): number {
    const v = o[key] as number;
    if (o._unit === 'mm') { const f = paperFrame(); return f.sc > 0 ? f.ox + v * f.sc : v; }
    return v;
  }
  function ovY(o: any, key: string): number {
    const v = o[key] as number;
    if (o._unit === 'mm') { const f = paperFrame(); return f.sc > 0 ? f.oy + v * f.sc : v; }
    return v;
  }

  function renderImages() {
    if (!imgLayer) return;
    imgLayer.innerHTML = '';
    const overlays = getOverlays() as any[];
    const strokes = getStrokes();
    const selIdx = getSelectedIdx();
    const selectedOverlay = (selIdx >= 0 && selIdx >= strokes.length) ? overlays[selIdx - strokes.length] : null;
    // Image-in-image model: ai-image's bounding rect = viewport (panel clip).
    // _imageX/_imageY (mm or px depending on _unit) translate the image content
    // inside the viewport; _imageScale (default 1) scales it.
    for (const o of overlays) {
      if ((o._visible as boolean) === false) continue;
      if (!isNodeVisible(o._nid as string)) continue;
      if ((o.type === 'ai-image') && (o._genImageUrl || o._genImage)) {
        const cx1 = ovX(o, 'x1'), cx2 = ovX(o, 'x2');
        const cy1 = ovY(o, 'y1'), cy2 = ovY(o, 'y2');
        const x1 = (Math.min(cx1, cx2) * zoom + panX) / dpr;
        const y1 = (Math.min(cy1, cy2) * zoom + panY) / dpr;
        const x2 = (Math.max(cx1, cx2) * zoom + panX) / dpr;
        const y2 = (Math.max(cy1, cy2) * zoom + panY) / dpr;
        const w = x2 - x1, h = y2 - y1;
        // Image content offset (in viewport CSS px). mm-aware via sc/dpr.
        let offX = (o._imageX as number) || 0;
        let offY = (o._imageY as number) || 0;
        const scale = (o._imageScale as number) || 1;
        if (o._unit === 'mm') {
          const f = paperFrame();
          if (f.sc > 0) { offX = offX * f.sc / dpr; offY = offY * f.sc / dpr; }
        }
        const srcUrlForDims = resolveImgUrl((o._genImageUrl as string) || '');
        const dims = srcUrlForDims ? getImgDims(srcUrlForDims) : null;
        // Source-aspect-aware cover sizing. If natural dims known, scale so
        // the image covers the wrapper (one axis fills exactly, the other
        // overflows). Apply user scale on top.
        let imgW: number, imgH: number;
        if (dims && dims.w > 0 && dims.h > 0) {
          const srcAspect = dims.w / dims.h;
          const wrapAspect = w / h;
          if (srcAspect > wrapAspect) {
            // Source wider than wrap — height fills wrap, width overflows
            imgH = h * scale;
            imgW = imgH * srcAspect;
          } else {
            // Source taller — width fills wrap, height overflows
            imgW = w * scale;
            imgH = imgW / srcAspect;
          }
        } else {
          // Fallback (dims still loading) — uniform scale, less pan range.
          imgW = w * scale;
          imgH = h * scale;
        }
        const minOffX = w - imgW;
        const minOffY = h - imgH;
        offX = Math.max(minOffX, Math.min(0, offX));
        offY = Math.max(minOffY, Math.min(0, offY));
        const isSelected = selectedOverlay === o;
        const srcUrl = resolveImgUrl((o._genImageUrl as string) || '') || ('data:image/jpeg;base64,' + o._genImage);
        // When selected, render a "ghost" image at the FULL image bounds with
        // low opacity so the user can see where the image extends beyond the
        // panel viewport while editing.
        if (isSelected) {
          const ghost = document.createElement('img');
          ghost.src = srcUrl;
          ghost.style.cssText = `position:absolute;left:${x1 + offX}px;top:${y1 + offY}px;width:${imgW}px;height:${imgH}px;object-fit:cover;pointer-events:none;opacity:0.25;outline:1px dashed #c0a040`;
          imgLayer.appendChild(ghost);
        }
        // Clipping wrapper sized to ai-image bounding rect (= panel viewport).
        const wrap = document.createElement('div');
        wrap.style.cssText = `position:absolute;left:${x1}px;top:${y1}px;width:${w}px;height:${h}px;overflow:hidden;pointer-events:none`;
        const el = document.createElement('img');
        el.src = srcUrl;
        el.style.cssText = `position:absolute;left:${offX}px;top:${offY}px;width:${imgW}px;height:${imgH}px;object-fit:cover;pointer-events:none;opacity:0.85`;
        el.onload = () => { (window as any).__imgLoaded = ((window as any).__imgLoaded || 0) + 1; };
        el.onerror = () => {
          (window as any).__imgFailed = ((window as any).__imgFailed || []);
          if ((window as any).__imgFailed.length < 5) (window as any).__imgFailed.push(srcUrl);
        };
        wrap.appendChild(el);
        imgLayer.appendChild(wrap);
      }
    }
  }

  // PDS rejects GET on /xrpc/com.atproto.sync.getBlob (POST-only). Rewrite
  // stored URLs to mangaka edge worker `/blob/:cid` proxy so <img src="..."> works.
  function resolveImgUrl(u: string): string {
    if (!u) return '';
    const m = u.match(/[?&]cid=([^&]+)/);
    if (m && u.includes('com.atproto.sync.getBlob')) {
      const cid = decodeURIComponent(m[1]);
      const didMatch = u.match(/[?&]did=([^&]+)/);
      const did = didMatch ? decodeURIComponent(didMatch[1]) : 'anonymous';
      return `/blob/${cid}?did=${encodeURIComponent(did)}`;
    }
    return u;
  }

  // Cache of source image natural dimensions, keyed by URL. Off-screen Image()
  // populates on first encounter; tracks pending/failed so we never re-issue
  // the request from inside the render loop (otherwise we'd flood the browser
  // with `new Image()` per frame for any URL whose load is in-flight or errored).
  const _imgDims = new Map<string, { w: number; h: number }>();
  const _imgPending = new Set<string>();
  function getImgDims(url: string): { w: number; h: number } | null {
    const hit = _imgDims.get(url);
    if (hit) return hit.w > 0 && hit.h > 0 ? hit : null;
    if (_imgPending.has(url)) return null;
    _imgPending.add(url);
    const img = new Image();
    img.onload = () => {
      _imgPending.delete(url);
      _imgDims.set(url, { w: img.naturalWidth, h: img.naturalHeight });
      requestRedraw();
    };
    img.onerror = () => {
      _imgPending.delete(url);
      _imgDims.set(url, { w: 0, h: 0 }); // sentinel: load failed, fall back forever
    };
    img.src = url;
    return null;
  }

  const FONT_FAMILIES: Record<string, string> = {
    gothic: '"Noto Sans JP", "Hiragino Kaku Gothic ProN", sans-serif',
    mincho: '"Noto Serif JP", "Hiragino Mincho ProN", serif',
    maru: '"M PLUS Rounded 1c", "Mochiy Pop One", sans-serif',
    handwritten: '"Yusei Magic", "Klee One", "Caveat", cursive',
    sfx: '"Reggae One", "Bungee", "Mochiy Pop One", sans-serif',
  };

  function renderTexts() {
    if (!textLayer) return;
    textLayer.innerHTML = '';
    const overlays = getOverlays();
    for (const o of overlays) {
      if (o.type !== 'text' && o.type !== 'link') continue;
      if (!isNodeVisible(o._nid as string)) continue;
      const cx = ovX(o, 'x'), cy = ovY(o, 'y');
      const el = document.createElement('div');
      // For mm-unit text, fontSize is also mm; convert to CSS pixels via sc/dpr.
      let fs = (o.fontSize as number) || 20;
      if (o._unit === 'mm') { const f = paperFrame(); if (f.sc > 0) fs = fs * f.sc / dpr; }
      const fontFamily = FONT_FAMILIES[(o.fontFamily as string) || ''] || FONT_FAMILIES.gothic;
      const style = (o.fontStyle as string) || 'normal';
      const fw = (style === 'bold' || style === 'bolditalic') ? '900' : '700';
      const fi = (style === 'italic' || style === 'bolditalic') ? 'italic' : 'normal';
      const isSfx = !!o.isSfx || (o.fontFamily === 'sfx');
      const stroke = isSfx ? 'text-shadow:0 0 2px #fff,2px 2px 0 #fff,-2px -2px 0 #fff,2px -2px 0 #fff,-2px 2px 0 #fff;' : '';
      el.style.cssText = `position:absolute;left:${cx / dpr}px;top:${cy / dpr}px;font-size:${fs}px;font-family:${fontFamily};font-weight:${fw};font-style:${fi};color:${o.color || '#000'};line-height:1.1;white-space:pre;pointer-events:none;${stroke}`;
      el.textContent = o.text as string;
      textLayer.appendChild(el);
    }
  }

  // Hit-test overlays at canvas-internal pixel position (cx, cy).
  // Returns overlays index, or -1 if none.
  function hitTest(cx: number, cy: number): number {
    const overlays = getOverlays();
    // Top of z-order first: text > fukidashi > ai-image > panel/tone.
    const orderPrio = (t: string) => (t === 'text' ? 4 : t === 'fukidashi' ? 3 : t === 'ai-image' ? 2 : t === 'panel' || t === 'tone' ? 1 : 0);
    const indices: number[] = [];
    for (let i = 0; i < overlays.length; i++) indices.push(i);
    indices.sort((a, b) => orderPrio((overlays[b].type as string) || '') - orderPrio((overlays[a].type as string) || ''));
    for (const i of indices) {
      const o = overlays[i];
      if ((o._visible as boolean) === false) continue;
      if (!isNodeVisible(o._nid as string)) continue;
      const t = (o.type as string) || '';
      if (t === 'panel' || t === 'tone' || t === 'fukidashi' || t === 'ai-image') {
        const cx1 = ovX(o, 'x1'), cx2 = ovX(o, 'x2');
        const cy1 = ovY(o, 'y1'), cy2 = ovY(o, 'y2');
        const lo_x = Math.min(cx1, cx2), hi_x = Math.max(cx1, cx2);
        const lo_y = Math.min(cy1, cy2), hi_y = Math.max(cy1, cy2);
        if (cx >= lo_x && cx <= hi_x && cy >= lo_y && cy <= hi_y) return i;
      } else if (t === 'text') {
        const x = ovX(o, 'x'), y = ovY(o, 'y');
        const fs = ((o.fontSize as number) || 5);
        const f = paperFrame();
        const wPx = (o._unit === 'mm' && f.sc > 0) ? fs * f.sc * (String(o.text || '').length || 1) * 0.7 : fs * (String(o.text || '').length || 1) * 0.7;
        const hPx = (o._unit === 'mm' && f.sc > 0) ? fs * f.sc * 1.4 : fs * 1.4;
        if (cx >= x && cx <= x + wPx && cy >= y && cy <= y + hPx) return i;
      }
    }
    return -1;
  }

  // Drag state — set on pointerdown over a hit, cleared on pointerup.
  let drag: {
    overlayIdx: number;
    startClientX: number;
    startClientY: number;
    initial: { x?: number; y?: number; x1?: number; y1?: number; x2?: number; y2?: number };
    children: Array<{ overlay: any; initial: any }>;
  } | null = null;

  function onCanvasPointerDown(e: PointerEvent) {
    if (activeMode !== 'select') return;
    const rect = canvasEl.getBoundingClientRect();
    const cssX = e.clientX - rect.left;
    const cssY = e.clientY - rect.top;
    const cx = cssX * dpr;
    const cy = cssY * dpr;
    // Face-add mode: clicking inside an ai-image adds a face marker (normalized
    // coords 0-1 within the image rect). Other types are ignored in this mode.
    if (faceAddMode) {
      const overlays = getOverlays() as any[];
      // Iterate ai-images in z-order (top-most first via reverse).
      for (let i = overlays.length - 1; i >= 0; i--) {
        const ai = overlays[i];
        if (ai.type !== 'ai-image') continue;
        const ix1 = ovX(ai, 'x1'), ix2 = ovX(ai, 'x2');
        const iy1 = ovY(ai, 'y1'), iy2 = ovY(ai, 'y2');
        const lox = Math.min(ix1, ix2), hix = Math.max(ix1, ix2);
        const loy = Math.min(iy1, iy2), hiy = Math.max(iy1, iy2);
        if (cx >= lox && cx <= hix && cy >= loy && cy <= hiy) {
          const fcx = (cx - lox) / Math.max(1, hix - lox);
          const fcy = (cy - loy) / Math.max(1, hiy - loy);
          onfaceadd?.(ai._nid as string, fcx, fcy);
          e.preventDefault();
          e.stopPropagation();
          return;
        }
      }
      return;
    }
    const overlayIdx = hitTest(cx, cy);
    const strokes = getStrokes();
    if (overlayIdx >= 0) {
      const idx = strokes.length + overlayIdx;
      setSelectedIdx(idx);
      onselect?.(idx);
      // Begin drag — snapshot the selected node + its children.
      const overlays = getOverlays();
      const o = overlays[overlayIdx] as any;
      // Auto-bump ai-image scale so first drag has room to pan inside the viewport.
      if (o.type === 'ai-image' && (!o._imageScale || o._imageScale <= 1)) {
        o._imageScale = 1.2;
      }
      const snap = (n: any) => ({
        x: n.x, y: n.y, x1: n.x1, y1: n.y1, x2: n.x2, y2: n.y2,
        _imageX: n._imageX, _imageY: n._imageY,
      });
      // Children move only if dragging a non-ai-image parent (panel/etc.).
      // Dragging an ai-image stays inside its viewport (panel stays put).
      const children = o.type === 'ai-image'
        ? []
        : (overlays as any[]).filter((c) => c._parent === o._nid).map((c) => ({ overlay: c, initial: snap(c) }));
      drag = {
        overlayIdx,
        startClientX: e.clientX,
        startClientY: e.clientY,
        initial: snap(o),
        children,
      };
      canvasEl.setPointerCapture(e.pointerId);
      e.preventDefault();
    } else {
      setSelectedIdx(-1);
      onselect?.(-1);
    }
    requestRedraw();
  }

  function onCanvasPointerMove(e: PointerEvent) {
    if (!drag) return;
    const overlays = getOverlays();
    const o = overlays[drag.overlayIdx];
    if (!o) return;
    const dxCss = e.clientX - drag.startClientX;
    const dyCss = e.clientY - drag.startClientY;
    const nid = (o as any)._nid as string;
    if ((o as any).type === 'ai-image') {
      // image_offset op — pan content within the panel viewport
      runop?.({ kind: 'image_offset', nid, initial: drag.initial as any, dxCss, dyCss, quiet: true });
    } else {
      // drag op — whole-node translate; panels carry their children along
      runop?.({
        kind: 'drag', nid, initial: drag.initial as any,
        childrenInitial: drag.children.map((c) => ({ nid: (c.overlay as any)._nid as string, initial: c.initial as any })),
        dxCss, dyCss, quiet: true,
      });
    }
    onmove?.();
    e.preventDefault();
  }

  function onCanvasPointerUp(e: PointerEvent) {
    if (!drag) return;
    const overlays = getOverlays();
    const o = overlays[drag.overlayIdx];
    const after: any = o ? {
      x: o.x, y: o.y, x1: o.x1, y1: o.y1, x2: o.x2, y2: o.y2,
      _imageX: (o as any)._imageX, _imageY: (o as any)._imageY,
    } : {};
    const info = {
      nid: (o?._nid as string) || '',
      type: (o?.type as string) || '',
      before: drag.initial,
      after,
    };
    drag = null;
    try { canvasEl.releasePointerCapture(e.pointerId); } catch { /* */ }
    ondragend?.(info);
  }

  // Render 8 resize handles around the selected rect-based node (panel/fukidashi/
  // ai-image/tone). Each handle has a `data-h` attribute identifying its corner
  // or edge (nw/n/ne/w/e/sw/s/se), which the pointerdown handler reads to
  // determine resize direction.
  // Per-type handle color so selection is visually distinguishable even when
  // ai-image / panel rects coincide (initial import state shares the same rect).
  const TYPE_COLORS: Record<string, { fill: string; border: string; label: string }> = {
    'ai-image':  { fill: '#fff3d6', border: '#d09030', label: 'AI Image' },
    'panel':     { fill: '#d6e6ff', border: '#3070d0', label: 'Panel' },
    'fukidashi': { fill: '#e6ffe6', border: '#40a040', label: 'Fukidashi' },
    'text':      { fill: '#f0e0ff', border: '#8040c0', label: 'Text' },
    'link':      { fill: '#f0e0ff', border: '#8040c0', label: 'Link' },
    'tone':      { fill: '#eeeeee', border: '#666666', label: 'Tone' },
    'group':     { fill: '#ffe0e0', border: '#c04040', label: 'Group' },
    'stroke':    { fill: '#ffffff', border: '#e06090', label: 'Stroke' },
  };
  function colorFor(type: string) {
    return TYPE_COLORS[type] || { fill: '#fff', border: '#e06090', label: type || 'Node' };
  }

  function renderHandles() {
    if (!handleLayer) return;
    handleLayer.innerHTML = '';
    const overlays = getOverlays() as any[];
    const strokes = getStrokes();
    const selIdx = getSelectedIdx();
    if (selIdx < 0 || selIdx < strokes.length) return;
    const o = overlays[selIdx - strokes.length];
    if (!o) return;
    if (o.x1 == null || o.x2 == null || o.y1 == null || o.y2 == null) return;
    const cx1 = ovX(o, 'x1'), cx2 = ovX(o, 'x2');
    const cy1 = ovY(o, 'y1'), cy2 = ovY(o, 'y2');
    const x1 = Math.min(cx1, cx2) / dpr, x2 = Math.max(cx1, cx2) / dpr;
    const y1 = Math.min(cy1, cy2) / dpr, y2 = Math.max(cy1, cy2) / dpr;
    const type = (o.type as string) || '';
    const c = colorFor(type);
    const positions: Array<{ h: string; x: number; y: number; cursor: string }> = [
      { h: 'nw', x: x1,           y: y1,           cursor: 'nwse-resize' },
      { h: 'n',  x: (x1 + x2) / 2, y: y1,           cursor: 'ns-resize'   },
      { h: 'ne', x: x2,           y: y1,           cursor: 'nesw-resize' },
      { h: 'w',  x: x1,           y: (y1 + y2) / 2, cursor: 'ew-resize'   },
      { h: 'e',  x: x2,           y: (y1 + y2) / 2, cursor: 'ew-resize'   },
      { h: 'sw', x: x1,           y: y2,           cursor: 'nesw-resize' },
      { h: 's',  x: (x1 + x2) / 2, y: y2,           cursor: 'ns-resize'   },
      { h: 'se', x: x2,           y: y2,           cursor: 'nwse-resize' },
    ];
    const HSZ = 8;
    for (const p of positions) {
      const el = document.createElement('div');
      el.dataset.h = p.h;
      el.style.cssText =
        `position:absolute;left:${p.x - HSZ / 2}px;top:${p.y - HSZ / 2}px;` +
        `width:${HSZ}px;height:${HSZ}px;background:${c.fill};border:1.5px solid ${c.border};` +
        `border-radius:2px;cursor:${p.cursor};pointer-events:auto;z-index:20`;
      handleLayer.appendChild(el);
    }
    // Face markers — render small dots on every visible ai-image's detected faces.
    // Click a dot to anchor the *most recently selected fukidashi*'s tail to that face.
    // Stored on the ai-image as `_faces: [{cx, cy, label?}]` with normalized 0-1 coords.
    const overlays_all = getOverlays() as any[];
    for (let i = 0; i < overlays_all.length; i++) {
      const ai = overlays_all[i];
      if (ai.type !== 'ai-image') continue;
      const faces = (ai._faces as Array<{ cx: number; cy: number; label?: string }> | undefined);
      if (!faces || faces.length === 0) continue;
      const ix1 = ovX(ai, 'x1') / dpr, ix2 = ovX(ai, 'x2') / dpr;
      const iy1 = ovY(ai, 'y1') / dpr, iy2 = ovY(ai, 'y2') / dpr;
      const w = Math.abs(ix2 - ix1), h = Math.abs(iy2 - iy1);
      const x0 = Math.min(ix1, ix2), y0 = Math.min(iy1, iy2);
      for (let fi = 0; fi < faces.length; fi++) {
        const f = faces[fi];
        const px = x0 + f.cx * w, py = y0 + f.cy * h;
        const dot = document.createElement('div');
        dot.dataset.h = 'face';
        dot.dataset.imgNid = ai._nid as string;
        dot.dataset.faceIdx = String(fi);
        dot.style.cssText =
          `position:absolute;left:${px - 6}px;top:${py - 6}px;width:12px;height:12px;` +
          `background:rgba(255,80,180,0.7);border:2px solid #fff;border-radius:50%;` +
          `cursor:pointer;pointer-events:auto;z-index:23;` +
          `box-shadow:0 0 0 1px #ff50b4`;
        dot.title = f.label ? `顔: ${f.label}` : `顔 #${fi + 1}`;
        handleLayer.appendChild(dot);
      }
    }
    // Emotion chips — render a small pill in the top-left of every ai-image /
    // panel that carries an `_emotion.primary` field (set by the
    // `score_emotion` Pregel or by `compose_scene_3d`'s Hume overlay).
    // Mood-coloured border so a maintainer can spot tone clusters across a
    // page at a glance. Pointer-events: none so the chip never blocks
    // overlay drag/resize.
    const emotionColorByName: Record<string, string> = {
      joy: '#f5a623', excitement: '#f56423', gratitude: '#f5a623', relief: '#83c44b',
      sadness: '#5b7fb4', doubt: '#7a6fb4', anxiety: '#7a6fb4', fear: '#7a6fb4',
      anger: '#d04040', calm: '#83c44b',
    };
    for (let i = 0; i < overlays_all.length; i++) {
      const node = overlays_all[i];
      if (node.type !== 'ai-image' && node.type !== 'panel') continue;
      const emo = (node as any)._emotion;
      const primaryName = emo?.primary?.name;
      if (!primaryName) continue;
      const nx1 = ovX(node, 'x1') / dpr, nx2 = ovX(node, 'x2') / dpr;
      const ny1 = ovY(node, 'y1') / dpr;
      const x0 = Math.min(nx1, nx2);
      const chip = document.createElement('div');
      chip.dataset.h = 'emotion-chip';
      chip.dataset.imgNid = (node as any)._nid as string;
      const borderColor = emotionColorByName[primaryName] || '#888';
      const score = Number(emo.primary.score || 0).toFixed(2);
      chip.style.cssText =
        `position:absolute;left:${x0 + 4}px;top:${ny1 + 4}px;` +
        `background:rgba(255,255,255,0.92);border:1.5px solid ${borderColor};` +
        `border-radius:10px;padding:2px 6px;font:11px/1.3 ui-monospace,monospace;` +
        `color:#222;pointer-events:none;z-index:22;white-space:nowrap;` +
        `box-shadow:0 1px 2px rgba(0,0,0,0.15)`;
      chip.textContent = `${primaryName} ${score}`;
      const algo = emo.algorithm || 'visual_heuristic_v1';
      const src = emo.sourceCount && emo.sourceCount > 1
        ? ` · ${emo.sourceCount} children (winner ${emo.winningChild ?? '?'})`
        : '';
      chip.title = `${algo}${src} · scoredAt ${emo.scoredAt ?? '?'}`;
      handleLayer.appendChild(chip);
    }

    // Fukidashi tail handle (purple diamond at _tailX/_tailY). Drag = move tail tip.
    // If tail not set yet, show a "+ tail" stub below the bubble suggesting where it'd appear.
    if (type === 'fukidashi') {
      const tailX = (o._tailX as number | undefined);
      const tailY = (o._tailY as number | undefined);
      const tHandle = document.createElement('div');
      tHandle.dataset.h = 'tail';
      if (tailX != null && tailY != null) {
        const tx = ovX(o, '_tailX') / dpr, ty = ovY(o, '_tailY') / dpr;
        tHandle.style.cssText =
          `position:absolute;left:${tx - 7}px;top:${ty - 7}px;width:14px;height:14px;` +
          `background:#9040ff;border:2px solid #fff;border-radius:50%;` +
          `cursor:move;pointer-events:auto;z-index:22;box-shadow:0 0 0 1px #9040ff`;
        tHandle.title = 'しっぽ (ドラッグで移動)';
      } else {
        const stubX = (x1 + x2) / 2, stubY = y2 + 12;
        tHandle.style.cssText =
          `position:absolute;left:${stubX - 8}px;top:${stubY - 8}px;width:16px;height:16px;` +
          `background:#fff;border:2px dashed #9040ff;border-radius:50%;color:#9040ff;` +
          `font:bold 12px/12px sans-serif;text-align:center;cursor:pointer;` +
          `pointer-events:auto;z-index:22`;
        tHandle.textContent = '+';
        tHandle.title = 'クリックでしっぽ追加';
      }
      handleLayer.appendChild(tHandle);
    }
    // Type label — small chip pinned to the NW corner of the selection rect.
    const lbl = document.createElement('div');
    lbl.style.cssText =
      `position:absolute;left:${x1}px;top:${y1 - 22}px;` +
      `padding:2px 6px;font:11px/1.2 -apple-system,sans-serif;` +
      `background:${c.border};color:#fff;border-radius:4px 4px 4px 0;` +
      `box-shadow:0 1px 3px rgba(0,0,0,0.3);pointer-events:none;z-index:21;` +
      `white-space:nowrap`;
    lbl.textContent = `${c.label}  ${Math.round(Math.abs(((o.x2 as number) - (o.x1 as number))))}×${Math.round(Math.abs(((o.y2 as number) - (o.y1 as number))))}${o._unit === 'mm' ? 'mm' : 'px'}`;
    handleLayer.appendChild(lbl);
  }

  // Resize state
  let resizing: {
    overlayIdx: number;
    handle: string;
    startClientX: number; startClientY: number;
    initial: any;
    children: Array<{ overlay: any; initial: any }>;
  } | null = null;

  function onHandlePointerDown(e: PointerEvent) {
    if (activeMode !== 'select') return;
    const t = e.target as HTMLElement | null;
    if (!t || !t.dataset || !t.dataset.h) return;
    const overlays = getOverlays() as any[];
    const strokes = getStrokes();
    const selIdx = getSelectedIdx();
    if (selIdx < 0 || selIdx < strokes.length) return;
    const oIdx = selIdx - strokes.length;
    const o = overlays[oIdx];
    if (!o) return;
    // Face dot click — anchor the currently selected fukidashi's tail to this face.
    if (t.dataset.h === 'face') {
      const imgNid = t.dataset.imgNid || '';
      const faceIdx = parseInt(t.dataset.faceIdx || '0', 10);
      if ((o as any).type === 'fukidashi') {
        runop?.({ kind: 'tail_anchor', nid: (o as any)._nid as string, imageNid: imgNid, faceIdx });
      }
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    // Tail handle (fukidashi): + stub click → seed tail below bubble; drag = move tip.
    if (t.dataset.h === 'tail' && (o as any).type === 'fukidashi') {
      if (o._tailX == null || o._tailY == null) {
        const bx = ((o.x1 as number) + (o.x2 as number)) / 2;
        const by = Math.max(o.y1 as number, o.y2 as number) + (o._unit === 'mm' ? 6 : 30);
        runop?.({ kind: 'update_props', nid: (o as any)._nid as string, patch: { _tailX: bx, _tailY: by } });
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      // Begin drag — store tail snapshot.
      resizing = {
        overlayIdx: oIdx,
        handle: 'tail',
        startClientX: e.clientX, startClientY: e.clientY,
        initial: { _tailX: o._tailX, _tailY: o._tailY } as any,
        children: [],
      };
      try { (t as any).setPointerCapture(e.pointerId); } catch { /* */ }
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    const snap = (n: any) => ({ x1: n.x1, y1: n.y1, x2: n.x2, y2: n.y2, _imageX: n._imageX, _imageY: n._imageY, _imageScale: n._imageScale });
    // ONLY panels cascade their resize to children. Other selectable nodes
    // (fukidashi / text / tone / ai-image) resize independently — they may
    // happen to overlap the panel/image rect, but their handles must not pull
    // siblings along.
    const children = o.type === 'panel'
      ? overlays.filter((c) => c._parent === o._nid).map((c) => ({ overlay: c, initial: snap(c) }))
      : [];
    resizing = {
      overlayIdx: oIdx,
      handle: t.dataset.h,
      startClientX: e.clientX, startClientY: e.clientY,
      initial: snap(o),
      children,
    };
    try { (t as any).setPointerCapture(e.pointerId); } catch { /* */ }
    e.preventDefault();
    e.stopPropagation();
  }

  function onHandlePointerMove(e: PointerEvent) {
    if (!resizing) return;
    const overlays = getOverlays() as any[];
    const o = overlays[resizing.overlayIdx];
    if (!o) return;
    const dxCss = e.clientX - resizing.startClientX;
    const dyCss = e.clientY - resizing.startClientY;
    let dx = dxCss * dpr, dy = dyCss * dpr;
    if (o._unit === 'mm') {
      const f = paperFrame();
      if (f.sc > 0) { dx = dxCss * dpr / f.sc; dy = dyCss * dpr / f.sc; }
    }
    const h = resizing.handle;
    const init = resizing.initial;

    // Tail handle: translate the tail tip via tail_drag Pregel op.
    if (h === 'tail') {
      runop?.({
        kind: 'tail_drag',
        nid: (o as any)._nid as string,
        initial: { _tailX: init._tailX as number | null, _tailY: init._tailY as number | null },
        dxCss: e.clientX - resizing.startClientX,
        dyCss: e.clientY - resizing.startClientY,
        quiet: true,
      });
      onmove?.();
      e.preventDefault();
      return;
    }

    // Resize via Pregel pipeline. quiet=true skips the audit emit step —
    // ondragend (pointerup) records the final before/after via its own op.
    runop?.({
      kind: 'resize',
      nid: (o as any)._nid as string,
      handle: h as any,
      initial: init,
      dxCss: e.clientX - resizing.startClientX,
      dyCss: e.clientY - resizing.startClientY,
      quiet: true,
    });
    onmove?.();
    e.preventDefault();
  }

  function onHandlePointerUp(e: PointerEvent) {
    if (!resizing) return;
    const overlays = getOverlays() as any[];
    const o = overlays[resizing.overlayIdx];
    const after: any = o ? { x1: o.x1, y1: o.y1, x2: o.x2, y2: o.y2 } : {};
    const info = {
      nid: (o?._nid as string) || '',
      type: (o?.type as string) || '',
      before: resizing.initial,
      after,
    };
    resizing = null;
    ondragend?.(info);
    e.preventDefault();
  }

  function renderFukidashi() {
    if (!fukidashiLayer) return;
    const overlays = getOverlays();
    const rect = canvasEl.getBoundingClientRect();
    fukidashiLayer.setAttribute('width', String(rect.width));
    fukidashiLayer.setAttribute('height', String(rect.height));
    fukidashiLayer.setAttribute('viewBox', `0 0 ${rect.width} ${rect.height}`);
    const ns = 'http://www.w3.org/2000/svg';
    fukidashiLayer.innerHTML = '';
    for (const o of overlays) {
      if (o.type !== 'fukidashi') continue;
      if (!isNodeVisible(o._nid as string)) continue;
      const cx1 = ovX(o, 'x1') / dpr, cx2 = ovX(o, 'x2') / dpr;
      const cy1 = ovY(o, 'y1') / dpr, cy2 = ovY(o, 'y2') / dpr;
      const cx = (cx1 + cx2) / 2, cy = (cy1 + cy2) / 2;
      const rx = Math.abs(cx2 - cx1) / 2, ry = Math.abs(cy2 - cy1) / 2;
      const shape = (o.shape as string) || 'normal';
      let node: SVGElement;
      if (shape === 'shout') {
        // Jagged starburst: 16-point star
        const pts: string[] = [];
        const N = 16;
        for (let i = 0; i < N * 2; i++) {
          const a = (i / (N * 2)) * Math.PI * 2 - Math.PI / 2;
          const r = (i % 2 === 0) ? 1.0 : 0.7;
          pts.push(`${cx + Math.cos(a) * rx * r},${cy + Math.sin(a) * ry * r}`);
        }
        node = document.createElementNS(ns, 'polygon');
        node.setAttribute('points', pts.join(' '));
      } else {
        node = document.createElementNS(ns, 'ellipse');
        node.setAttribute('cx', String(cx));
        node.setAttribute('cy', String(cy));
        node.setAttribute('rx', String(rx));
        node.setAttribute('ry', String(ry));
      }
      node.setAttribute('fill', '#ffffff');
      node.setAttribute('stroke', '#000000');
      node.setAttribute('stroke-width', '2');
      if (shape === 'thought') node.setAttribute('stroke-dasharray', '4 4');
      else if (shape === 'whisper') node.setAttribute('stroke-dasharray', '2 3');
      fukidashiLayer.appendChild(node);

      // Tail (しっぽ) — small pointer from bubble edge to target.
      // Two anchor modes:
      //   (a) free position: _tailX / _tailY in same units as x1/y1 (mm or px)
      //   (b) face anchor: _tailAnchor = { imageNid, faceIdx } — derived per-frame
      //       from the referenced ai-image's rect + face's normalized coords.
      // 'thought' shape uses 3 shrinking circles instead of a triangle (manga convention).
      let tx: number | null = null, ty: number | null = null;
      const anchor = (o._tailAnchor as { imageNid?: string; faceIdx?: number } | undefined);
      if (anchor && anchor.imageNid != null && anchor.faceIdx != null) {
        const targetImg = findByNid(anchor.imageNid) as any;
        const faces = (targetImg?._faces as Array<{ cx: number; cy: number }> | undefined);
        const face = faces?.[anchor.faceIdx];
        if (targetImg && face) {
          const ix1 = ovX(targetImg, 'x1') / dpr, ix2 = ovX(targetImg, 'x2') / dpr;
          const iy1 = ovY(targetImg, 'y1') / dpr, iy2 = ovY(targetImg, 'y2') / dpr;
          const w = Math.abs(ix2 - ix1), h = Math.abs(iy2 - iy1);
          tx = Math.min(ix1, ix2) + face.cx * w;
          ty = Math.min(iy1, iy2) + face.cy * h;
        }
      }
      if (tx == null || ty == null) {
        const tailX = (o._tailX as number | undefined);
        const tailY = (o._tailY as number | undefined);
        if (tailX != null && tailY != null) {
          tx = ovX(o, '_tailX') / dpr;
          ty = ovY(o, '_tailY') / dpr;
        }
      }
      if (tx != null && ty != null && shape !== 'shout') {
        const angle = Math.atan2(ty - cy, tx - cx);
        // Base on bubble edge nearest the target
        const edgeX = cx + Math.cos(angle) * rx;
        const edgeY = cy + Math.sin(angle) * ry;
        if (shape === 'thought') {
          // Mini bubble trail: 3 circles, shrinking, between bubble edge and target.
          const dx = tx - edgeX, dy = ty - edgeY;
          for (let i = 1; i <= 3; i++) {
            const t = i / 4;
            const px = edgeX + dx * t, py = edgeY + dy * t;
            const r = Math.max(2, ry * 0.18 * (1 - i * 0.2));
            const c = document.createElementNS(ns, 'circle');
            c.setAttribute('cx', String(px));
            c.setAttribute('cy', String(py));
            c.setAttribute('r', String(r));
            c.setAttribute('fill', '#ffffff');
            c.setAttribute('stroke', '#000000');
            c.setAttribute('stroke-width', '2');
            fukidashiLayer.appendChild(c);
          }
        } else {
          // Triangle tail. Base width ~25% of nearest radius, narrowing to target.
          const baseHalf = Math.min(rx, ry) * 0.28;
          const perpA = angle + Math.PI / 2;
          const b1x = edgeX + Math.cos(perpA) * baseHalf;
          const b1y = edgeY + Math.sin(perpA) * baseHalf;
          const b2x = edgeX - Math.cos(perpA) * baseHalf;
          const b2y = edgeY - Math.sin(perpA) * baseHalf;
          const tri = document.createElementNS(ns, 'polygon');
          tri.setAttribute('points', `${b1x},${b1y} ${b2x},${b2y} ${tx},${ty}`);
          tri.setAttribute('fill', '#ffffff');
          tri.setAttribute('stroke', '#000000');
          tri.setAttribute('stroke-width', '2');
          tri.setAttribute('stroke-linejoin', 'round');
          if (shape === 'whisper') tri.setAttribute('stroke-dasharray', '2 3');
          fukidashiLayer.appendChild(tri);
          // Erase the inner segment of the ellipse outline that the triangle crosses.
          // Draw a slightly inset white line between b1 and b2 along the bubble edge.
          const eraser = document.createElementNS(ns, 'line');
          eraser.setAttribute('x1', String(b1x));
          eraser.setAttribute('y1', String(b1y));
          eraser.setAttribute('x2', String(b2x));
          eraser.setAttribute('y2', String(b2y));
          eraser.setAttribute('stroke', '#ffffff');
          eraser.setAttribute('stroke-width', '3');
          fukidashiLayer.appendChild(eraser);
        }
      }
      if (o.text) {
        // Vertical Japanese (tategaki, right-to-left columns). SVG <text> doesn't
        // do vertical-rl well across browsers, so embed an HTML div via foreignObject.
        // Horizontal mode via `_textOrientation === 'horizontal'` override.
        const txt = String(o.text);
        const vertical = (o._textOrientation as string) !== 'horizontal';
        const maxW = rx * 1.7, maxH = ry * 1.6;
        // For vertical: rows = chars per column ≈ maxH / fs;   cols = lines (right-to-left)
        // For horizontal: chars per line ≈ maxW / fs;          lines stacked top-to-bottom
        const estimatedChars = txt.length;
        let fs: number;
        if (vertical) {
          fs = Math.min(rx * 0.55, (maxH / Math.max(1, Math.ceil(estimatedChars / 3))) * 0.9);
        } else {
          fs = Math.min(ry * 0.55, (maxW / Math.max(1, Math.ceil(estimatedChars / 3))) * 0.9);
        }
        fs = Math.max(7, Math.min(fs, 22));

        const fo = document.createElementNS(ns, 'foreignObject');
        const boxW = 2 * rx, boxH = 2 * ry;
        fo.setAttribute('x', String(cx - rx));
        fo.setAttribute('y', String(cy - ry));
        fo.setAttribute('width', String(boxW));
        fo.setAttribute('height', String(boxH));
        const inner = document.createElement('div');
        inner.setAttribute('xmlns', 'http://www.w3.org/1999/xhtml');
        const wm = vertical ? 'vertical-rl' : 'horizontal-tb';
        const orient = vertical ? 'text-orientation:mixed;' : '';
        inner.style.cssText =
          `width:100%;height:100%;display:flex;align-items:center;justify-content:center;` +
          `font-family:${FONT_FAMILIES.gothic};font-size:${fs}px;color:#000;line-height:1.15;` +
          `writing-mode:${wm};${orient}` +
          `overflow:hidden;text-align:center;padding:${Math.max(2, fs*0.2)}px;box-sizing:border-box;` +
          `pointer-events:none;white-space:normal;word-break:break-all`;
        inner.textContent = txt;
        fo.appendChild(inner);
        fukidashiLayer.appendChild(fo);
      }
    }
  }

  function isNodeVisible(nid: string): boolean {
    let cur = nid;
    const visited = new Set<string>();
    while (cur) {
      if (visited.has(cur)) return true;
      visited.add(cur);
      const n = findByNid(cur);
      if (!n) return true;
      if (n._visible === false) return false;
      cur = (n._parent as string) || '';
    }
    return true;
  }

  function render() {
    const hasRedraw = consumeRedraw();
    if (!hasRedraw && !isDrawing) { animId = requestAnimationFrame(render); return; }
    if (!renderFrame) { animId = requestAnimationFrame(render); return; }

    const data = tessellateAll();
    const vertCount = data.length / 6;
    renderFrame(data, vertCount);
    renderTexts();
    renderImages();
    renderFukidashi();
    renderHandles();
    animId = requestAnimationFrame(render);
  }

  onMount(async () => {
    dpr = devicePixelRatio || 1;
    canvasEl.width = (width || canvasEl.clientWidth) * dpr;
    canvasEl.height = (height || canvasEl.clientHeight) * dpr;
    try {
      await initGPU();
      gpuError = '';
      requestRedraw();
      animId = requestAnimationFrame(render);
    } catch (e: any) {
      console.error('WebGPU init failed:', e);
      try {
        initWebGL();
        gpuError = '';
        requestRedraw();
        animId = requestAnimationFrame(render);
      } catch (webglError: any) {
        renderBackend = 'none';
        renderFrame = null;
        gpuError = webglError?.message ?? e?.message ?? String(webglError ?? e);
        console.error('WebGL init failed:', webglError);
      }
    }
  });

  onDestroy(() => { if (animId) cancelAnimationFrame(animId); });

  // Wheel-to-zoom (Ctrl/Cmd + wheel) and pinch-to-zoom (wheel with ctrlKey from trackpad).
  // Plain wheel = vertical pan. Shift+wheel = horizontal pan.
  function onCanvasWheel(e: WheelEvent) {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      const factor = Math.exp(-e.deltaY * 0.005);
      const next = Math.max(0.1, Math.min(5, zoom * factor));
      // Zoom around the cursor: anchor world point under cursor stays put.
      const rect = canvasEl.getBoundingClientRect();
      const cx = (e.clientX - rect.left) * dpr;
      const cy = (e.clientY - rect.top) * dpr;
      panX = cx - (cx - panX) * (next / zoom);
      panY = cy - (cy - panY) * (next / zoom);
      zoom = next;
      onzoomchange?.(zoom);
      onpanchange?.(panX, panY);
      requestRedraw();
    } else {
      e.preventDefault();
      if (e.shiftKey) { panX -= e.deltaY; } else { panY -= e.deltaY; panX -= e.deltaX; }
      onpanchange?.(panX, panY);
      requestRedraw();
    }
  }
  // Programmatic zoom (called from toolbar buttons via prop callback).
  export function setZoom(z: number) {
    zoom = Math.max(0.1, Math.min(5, z));
    onzoomchange?.(zoom);
    requestRedraw();
  }
  export function fitToView() {
    const y = YOUSHI[activeYoushi];
    if (!y?.draw) return;
    const cssW = canvasEl.clientWidth, cssH = canvasEl.clientHeight;
    const paperW = y.wMM * 2.4, paperH = y.hMM * 2.4;
    const fit = Math.min(cssW / paperW, cssH / paperH) * 0.95;
    zoom = Math.max(0.1, Math.min(5, fit));
    panX = 0; panY = 0;
    onzoomchange?.(zoom);
    onpanchange?.(panX, panY);
    requestRedraw();
  }
</script>

<div class="canvas-wrap">
  <canvas bind:this={canvasEl} id="draw" onpointerdown={onCanvasPointerDown} onpointermove={onCanvasPointerMove} onpointerup={onCanvasPointerUp} onpointercancel={onCanvasPointerUp} onwheel={onCanvasWheel}></canvas>
  <div bind:this={imgLayer} class="img-layer"></div>
  <svg bind:this={fukidashiLayer} class="fukidashi-layer" xmlns="http://www.w3.org/2000/svg"></svg>
  <div bind:this={textLayer} class="text-layer"></div>
  <div bind:this={handleLayer} class="handle-layer" onpointerdown={onHandlePointerDown} onpointermove={onHandlePointerMove} onpointerup={onHandlePointerUp} onpointercancel={onHandlePointerUp}></div>
  {#if gpuError}
    <div class="gpu-fallback">
      <div class="gpu-card">
        <h2>Canvas unavailable</h2>
        <p>
          Mangaka uses WebGPU by default and falls back to WebGL when needed. The document tree
          is loaded, but neither graphics backend could start in this browser.
        </p>
        <p class="gpu-detail">{gpuError}</p>
      </div>
    </div>
  {/if}
</div>

<style>
  .canvas-wrap { position:relative; flex:1; overflow:hidden; background:linear-gradient(180deg, #20242d 0%, #16191f 100%); }
  canvas { width:100%; height:100%; display:block; }
  .img-layer, .text-layer, .fukidashi-layer, .handle-layer { position:absolute; top:0; left:0; width:100%; height:100%; pointer-events:none; z-index:4; }
  .fukidashi-layer { z-index:5; }
  .text-layer { z-index:6; }
  .handle-layer { z-index:20; pointer-events:none; }
  .gpu-fallback {
    position:absolute;
    inset:0;
    display:grid;
    place-items:center;
    padding:24px;
    z-index:6;
    background:rgba(10, 12, 16, 0.52);
  }
  .gpu-card {
    max-width:520px;
    padding:24px;
    border:1px solid rgba(255, 255, 255, 0.14);
    border-radius:20px;
    background:rgba(20, 24, 31, 0.88);
    color:#f5f7fb;
    box-shadow:0 18px 60px rgba(0, 0, 0, 0.28);
  }
  .gpu-card h2 {
    margin:0 0 10px;
    font-size:24px;
  }
  .gpu-card p {
    margin:0;
    line-height:1.6;
    color:#c2cbda;
  }
  .gpu-detail {
    margin-top:12px !important;
    font-family:ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size:13px;
    color:#ffd6bf !important;
  }
</style>
