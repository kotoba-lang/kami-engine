declare module 'https://cdn.jsdelivr.net/npm/@met4citizen/headtts@1.2/+esm' {
  export class HeadTTS {
    constructor(options: {
      endpoints?: string[];
      languages?: string[];
    });
    onmessage?: (message: unknown) => void;
    connect(): Promise<void>;
    setup(options: {
      voice?: string;
      speed?: number;
    }): void;
    synthesize(options: {
      input: string;
    }): Promise<void>;
  }
}

interface GPU {
  requestAdapter(options?: Record<string, unknown>): Promise<GPUAdapter | null>;
  getPreferredCanvasFormat(): GPUTextureFormat;
}

interface Navigator {
  gpu?: GPU;
}

interface GPUAdapter {
  requestDevice(descriptor?: Record<string, unknown>): Promise<GPUDevice>;
  requestAdapterInfo?(): Promise<GPUAdapterInfo>;
}

interface GPUAdapterInfo {
  vendor: string;
  architecture: string;
  device: string;
}

interface GPUDevice {
  queue: GPUQueue;
  createShaderModule(descriptor: { code: string }): GPUShaderModule;
  createRenderPipeline(descriptor: Record<string, unknown>): GPURenderPipeline;
  createBuffer(descriptor: { size: number; usage: number }): GPUBuffer;
  createBindGroup(descriptor: Record<string, unknown>): GPUBindGroup;
  createCommandEncoder(): GPUCommandEncoder;
}

interface GPUQueue {
  writeBuffer(buffer: GPUBuffer, bufferOffset: number, data: BufferSource | SharedArrayBuffer): void;
  submit(commandBuffers: GPUCommandBuffer[]): void;
}

interface GPUShaderModule {}

interface GPURenderPipeline {
  getBindGroupLayout(index: number): GPUBindGroupLayout;
}

interface GPUBindGroupLayout {}

interface GPUBuffer {}

interface GPUBindGroup {}

interface GPUCommandEncoder {
  beginRenderPass(descriptor: Record<string, unknown>): GPURenderPassEncoder;
  finish(): GPUCommandBuffer;
}

interface GPUCommandBuffer {}

interface GPURenderPassEncoder {
  setPipeline(pipeline: GPURenderPipeline): void;
  setBindGroup(index: number, bindGroup: GPUBindGroup): void;
  setVertexBuffer(slot: number, buffer: GPUBuffer): void;
  draw(vertexCount: number): void;
  end(): void;
}

interface GPUCanvasContext {
  configure(descriptor: Record<string, unknown>): void;
  getCurrentTexture(): GPUTexture;
}

interface GPUTexture {
  createView(): GPUTextureView;
}

interface GPUTextureView {}

type GPUTextureFormat = string;

// three.js was previously declared as an OPTIONAL peer dependency for the
// (now-removed) `spark/` and `webvr/webvr-scene.ts` modules. As of
// 2026-05-26 the SDK is three-free: 3DGS rendering goes through the
// canonical `GsplatAdapter` (Rust + wgpu, kami-pipelines crate) via the
// `gsplat/` WASM bridge, and webvr is a headless engine whose surface is
// owned by a `kami-app-{game}` crate. No `declare module 'three'` block
// is needed any more — keep this comment as a removal marker.

// Minimal WebXR shape — kept for downstream callers that still consume
// SceneDescriptor.cameraHint to drive their own WebXR surfaces.
// Apps with their own @types/webxr will override this via interface merging.
interface XRSession extends EventTarget {
  end(): Promise<void>;
}
interface XRSystem {
  isSessionSupported(mode: string): Promise<boolean>;
  requestSession(
    mode: string,
    init?: { optionalFeatures?: string[]; requiredFeatures?: string[] },
  ): Promise<XRSession>;
}
interface Navigator {
  xr?: XRSystem;
}

// Minimal SpeechSynthesis shim — present in all modern browsers but
// optional. The SDK reads through Window globals only and degrades to
// silent if missing. lib.dom already declares these on modern TS but
// kept here for the SDK's `types: ["svelte"]` tsconfig.
interface SpeechSynthesisUtterance {
  text: string;
  lang: string;
  rate: number;
  pitch: number;
  volume: number;
}
declare const SpeechSynthesisUtterance: {
  new (text?: string): SpeechSynthesisUtterance;
};
interface SpeechSynthesis {
  speak(u: SpeechSynthesisUtterance): void;
  cancel(): void;
  pause(): void;
  resume(): void;
  speaking: boolean;
  paused: boolean;
  pending: boolean;
}
interface Window {
  speechSynthesis?: SpeechSynthesis;
}

declare const GPUBufferUsage: {
  readonly COPY_DST: number;
  readonly UNIFORM: number;
  readonly VERTEX: number;
};
