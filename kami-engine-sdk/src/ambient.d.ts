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

// three is an OPTIONAL peer dependency (see package.json). Declare the
// subset of named exports used by webvr-scene.ts as `any`-classes so the
// SDK type-checks without requiring @types/three. Each class is both a
// value (constructor) and a type. Downstream consumers with @types/three
// installed get the real types via lib-augmentation.
declare module 'three' {
  /* eslint-disable @typescript-eslint/no-explicit-any */
  // Classes — usable both as `new THREE.X(...)` and as a TS type `THREE.X`.
  export class WebGLRenderer       { constructor(...args: any[]); [k: string]: any }
  export class Scene               { constructor(...args: any[]); [k: string]: any }
  export class PerspectiveCamera   { constructor(...args: any[]); [k: string]: any }
  export class Group               { constructor(...args: any[]); [k: string]: any }
  export class Mesh                { constructor(...args: any[]); [k: string]: any }
  export class Object3D            { constructor(...args: any[]); [k: string]: any }
  export class HemisphereLight     { constructor(...args: any[]); [k: string]: any }
  export class DirectionalLight    { constructor(...args: any[]); [k: string]: any }
  export class AmbientLight        { constructor(...args: any[]); [k: string]: any }
  export class SphereGeometry      { constructor(...args: any[]); [k: string]: any }
  export class Color               { constructor(...args: any[]); [k: string]: any }
  export class Fog                 { constructor(...args: any[]); [k: string]: any }
  export class CanvasTexture       { constructor(...args: any[]); [k: string]: any }
  export class Material            { constructor(...args: any[]); [k: string]: any }
  export class MeshBasicMaterial   { constructor(...args: any[]); [k: string]: any }
  export class MeshLambertMaterial { constructor(...args: any[]); [k: string]: any }
  export class MeshToonMaterial    { constructor(...args: any[]); [k: string]: any }
  export class MeshDepthMaterial   { constructor(...args: any[]); [k: string]: any }
  export class DataTexture         { constructor(...args: any[]); [k: string]: any }
  export class PlaneGeometry       { constructor(...args: any[]); [k: string]: any }
  export class BoxGeometry         { constructor(...args: any[]); [k: string]: any }
  export class CylinderGeometry    { constructor(...args: any[]); [k: string]: any }
  export class RingGeometry        { constructor(...args: any[]); [k: string]: any }
  export class BufferGeometry      { constructor(...args: any[]); [k: string]: any }
  export class Raycaster           { constructor(...args: any[]); [k: string]: any }
  export class Vector2             { constructor(...args: any[]); [k: string]: any }
  export class Vector3             { constructor(...args: any[]); [k: string]: any }
  export class Vector4             { constructor(...args: any[]); [k: string]: any }
  export class Quaternion          { constructor(...args: any[]); [k: string]: any }
  export class Euler               { constructor(...args: any[]); [k: string]: any }
  export class Matrix4             { constructor(...args: any[]); [k: string]: any }
  // Additional classes used by the `spark` sample suite.
  export class Points                    { constructor(...args: any[]); [k: string]: any }
  export class PointsMaterial            { constructor(...args: any[]); [k: string]: any }
  export class ShaderMaterial            { constructor(...args: any[]); [k: string]: any }
  export class RawShaderMaterial         { constructor(...args: any[]); [k: string]: any }
  export class BufferAttribute           { constructor(...args: any[]); [k: string]: any }
  export class Float32BufferAttribute    { constructor(...args: any[]); [k: string]: any }
  export class Uint16BufferAttribute     { constructor(...args: any[]); [k: string]: any }
  export class InstancedMesh             { constructor(...args: any[]); [k: string]: any }
  export class InstancedBufferAttribute  { constructor(...args: any[]); [k: string]: any }
  export class InstancedBufferGeometry   { constructor(...args: any[]); [k: string]: any }
  export class Clock                     { constructor(...args: any[]); [k: string]: any }
  export class Texture                   { constructor(...args: any[]); [k: string]: any }
  export class OrthographicCamera        { constructor(...args: any[]); [k: string]: any }
  // Enums / consts.
  export const BackSide: any;
  export const FrontSide: any;
  export const DoubleSide: any;
  export const SRGBColorSpace: any;
  export const AdditiveBlending: any;
  export const NormalBlending: any;
  export const DynamicDrawUsage: any;
  export const RGBAFormat: any;
  export const FloatType: any;
  export const ClampToEdgeWrapping: any;
  export const LinearFilter: any;
  export const NearestFilter: any;
}

// Minimal WebXR shape — keeps webvr-scene.ts type-checking without
// requiring @types/webxr. Apps with their own @types/webxr will
// override this via interface merging.
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
