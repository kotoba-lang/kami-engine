// post.wgsl — fullscreen post-processing chain (ADR-0044 phase 6 GPU wiring).
//
// Closes the "post-processing declared but not implemented" + "AA: none" gaps with
// real WGSL passes over the lit HDR target:
//   fs_bright    — threshold bright-pass (bloom source), half-res
//   fs_blur      — separable 9-tap Gaussian (run H then V)
//   fs_composite — HDR scene + bloom → exposure → ACES filmic tonemap → vignette → sRGB
//   fs_fxaa      — luma-based FXAA antialiasing on the LDR result
// Every pass shares one fullscreen-triangle vertex shader + one bind group layout
// (sampled texture + sampler + a small params uniform), so they pipeline cheaply.

struct P {
  p0: vec4<f32>,
  p1: vec4<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> p: P;

struct VO { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> VO {
  // single oversized triangle covering the viewport
  var o: VO;
  let x = f32((vi << 1u) & 2u);
  let y = f32(vi & 2u);
  o.uv = vec2<f32>(x, y);
  o.clip = vec4<f32>((x * 2.0 - 1.0), (1.0 - y * 2.0), 0.0, 1.0);
  return o;
}

fn luma(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.299, 0.587, 0.114)); }

// p0 = (threshold, knee, _, _)
@fragment
fn fs_bright(i: VO) -> @location(0) vec4<f32> {
  let c = textureSample(tex, samp, i.uv).rgb;
  let l = luma(c);
  let thr = p.p0.x;
  let soft = max(l - thr, 0.0);
  let k = soft / max(l, 0.0001);
  return vec4<f32>(c * k, 1.0);
}

// p0 = (dir.x, dir.y, texel.x, texel.y) — Gaussian σ≈2, 9 taps
@fragment
fn fs_blur(i: VO) -> @location(0) vec4<f32> {
  let dir = p.p0.xy * p.p0.zw;
  let w = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
  var acc = textureSample(tex, samp, i.uv).rgb * w[0];
  for (var k = 1; k < 5; k++) {
    let off = dir * f32(k);
    acc += textureSample(tex, samp, i.uv + off).rgb * w[k];
    acc += textureSample(tex, samp, i.uv - off).rgb * w[k];
  }
  return vec4<f32>(acc, 1.0);
}

fn aces(x: vec3<f32>) -> vec3<f32> {
  let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
  return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Second sampled texture = the bloom result, bound via a 2nd bind group at @group(1).
@group(1) @binding(0) var bloomTex: texture_2d<f32>;
@group(1) @binding(1) var bloomSamp: sampler;

// p0 = (exposure, bloom_strength, vignette, gamma)
@fragment
fn fs_composite(i: VO) -> @location(0) vec4<f32> {
  var hdr = textureSample(tex, samp, i.uv).rgb;
  let bloom = textureSample(bloomTex, bloomSamp, i.uv).rgb;
  hdr += bloom * p.p0.y;
  hdr *= p.p0.x;
  var c = aces(hdr);
  // vignette: darken toward the corners
  let d = i.uv - vec2<f32>(0.5);
  let vig = 1.0 - p.p0.z * dot(d, d) * 2.0;
  c *= clamp(vig, 0.0, 1.0);
  c = pow(c, vec3<f32>(1.0 / p.p0.w));
  return vec4<f32>(c, 1.0);
}

// p0 = (texel.x, texel.y, _, _) — FXAA 3.11-lite (edge-directed blend)
@fragment
fn fs_fxaa(i: VO) -> @location(0) vec4<f32> {
  let texel = p.p0.xy;
  let rgbM = textureSample(tex, samp, i.uv).rgb;
  let lM = luma(rgbM);
  let lNW = luma(textureSample(tex, samp, i.uv + vec2<f32>(-texel.x, -texel.y)).rgb);
  let lNE = luma(textureSample(tex, samp, i.uv + vec2<f32>( texel.x, -texel.y)).rgb);
  let lSW = luma(textureSample(tex, samp, i.uv + vec2<f32>(-texel.x,  texel.y)).rgb);
  let lSE = luma(textureSample(tex, samp, i.uv + vec2<f32>( texel.x,  texel.y)).rgb);
  let lMin = min(lM, min(min(lNW, lNE), min(lSW, lSE)));
  let lMax = max(lM, max(max(lNW, lNE), max(lSW, lSE)));
  if ((lMax - lMin) < (0.0312 + lMax * 0.125)) {
    return vec4<f32>(rgbM, 1.0);
  }
  var dir = vec2<f32>(
    -((lNW + lNE) - (lSW + lSE)),
    ((lNW + lSW) - (lNE + lSE)),
  );
  let reduce = max((lNW + lNE + lSW + lSE) * 0.03125, 0.0078125);
  let rcp = 1.0 / (min(abs(dir.x), abs(dir.y)) + reduce);
  dir = clamp(dir * rcp, vec2<f32>(-8.0), vec2<f32>(8.0)) * texel;
  let a = 0.5 * (
    textureSample(tex, samp, i.uv + dir * (1.0 / 3.0 - 0.5)).rgb +
    textureSample(tex, samp, i.uv + dir * (2.0 / 3.0 - 0.5)).rgb);
  let b = a * 0.5 + 0.25 * (
    textureSample(tex, samp, i.uv + dir * -0.5).rgb +
    textureSample(tex, samp, i.uv + dir * 0.5).rgb);
  let lB = luma(b);
  if ((lB < lMin) || (lB > lMax)) {
    return vec4<f32>(a, 1.0);
  }
  return vec4<f32>(b, 1.0);
}
