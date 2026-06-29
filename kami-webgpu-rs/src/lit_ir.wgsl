// lit_ir.wgsl — the render-IR forward lit pass (ADR-0044 phase 6 GPU wiring).
//
// Unlike lit_shader.wgsl (single hardcoded sun), this consumes the parsed RenderIr:
//   • up to MAX_LIGHTS lights (directional / point / spot) with range attenuation
//     and a smooth spot cone — closing the "directional-only" GPU gap;
//   • up to MAX_SHADOWS shadow-casting lights, each with its own matrix + atlas layer
//     (directional = ortho, spot = perspective) — closing "single directional shadow";
//   • Environment IBL — a hemisphere irradiance term (ground↔sky by N.y) plus a
//     procedural environment specular sampled in the reflection direction, scaled
//     by :env :ibl :intensity — closing the "no IBL / env map" GPU gap.
// Output is linear HDR (Rgba16Float); tonemapping + AA happen in post.wgsl.

const MAX_LIGHTS: u32 = 8u;
const MAX_SHADOWS: u32 = 4u;

struct Lgt {
  color: vec4<f32>,  // rgb, intensity
  pos:   vec4<f32>,  // xyz, range (point/spot)
  dir:   vec4<f32>,  // xyz dir, kind (0 dir / 1 point / 2 spot)
  spot:  vec4<f32>,  // cos_inner, cos_outer, _, shadow_layer (-1 = none)
};

struct G {
  vp:        mat4x4<f32>,
  shadow_vp: array<mat4x4<f32>, MAX_SHADOWS>,
  point_vp:  array<mat4x4<f32>, 6>,   // the 6 point-shadow face view-projections
  eye:       vec4<f32>,  // xyz eye, n_lights
  amb:       vec4<f32>,  // ambient rgb, ibl_intensity
  ground:    vec4<f32>,  // ground rgb, sky_mix weight
  sky:       vec4<f32>,  // sky/horizon rgb, _
  tune:      vec4<f32>,  // specStr, shininess, shadow_bias, texel
  lights:    array<Lgt, MAX_LIGHTS>,
};

@group(0) @binding(0) var<uniform> g: G;
@group(0) @binding(1) var shadowMap: texture_depth_2d_array;
@group(0) @binding(2) var shadowSamp: sampler_comparison;
// Point-light omnidirectional shadow: 6 faces of linear distance (frag→light)/range in
// a 2D-array (R16Float, filterable). The face is picked by the major axis of the
// light→fragment direction and projected with the matching `point_vp[face]` — the same
// matrix the cube was rendered with, so there is no cube-convention ambiguity. A light
// with shadow layer == POINT_LAYER (-2) uses this instead of the directional/spot atlas.
@group(0) @binding(3) var pointArr: texture_2d_array<f32>;
@group(0) @binding(4) var pointSamp: sampler;
// Image-based environment: an equirectangular HDR with a mip chain. Diffuse irradiance is
// the blurred top-mip along N; roughness-blurred specular is a `roughness*maxLod` sample
// along the reflection vector. Active when `sky.w > 0.5` (a real env map was uploaded);
// otherwise the procedural `env_radiance` gradient is used (1×1 fallback texture is bound).
@group(0) @binding(5) var envTex: texture_2d<f32>;
@group(0) @binding(6) var envSamp: sampler;

const POINT_LAYER: i32 = -2;
const PI: f32 = 3.14159265;

// World direction → equirectangular UV (longitude/latitude).
fn equirect_uv(d: vec3<f32>) -> vec2<f32> {
  let yaw = atan2(d.z, d.x);
  let pitch = acos(clamp(d.y, -1.0, 1.0));
  return vec2<f32>((yaw / (2.0 * PI)) + 0.5, pitch / PI);
}
fn env_sample(d: vec3<f32>, lod: f32) -> vec3<f32> {
  return textureSampleLevel(envTex, envSamp, equirect_uv(d), lod).rgb;
}

fn point_shadow(wpos: vec3<f32>, lpos: vec3<f32>, range: f32) -> f32 {
  let v = (wpos - lpos);
  let cur = (length(v) / max(range, 0.0001));
  let a = abs(v);
  var face = 0;                                  // face order +X,-X,+Y,-Y,+Z,-Z
  if ((a.x >= a.y) && (a.x >= a.z)) { if (v.x < 0.0) { face = 1; } }
  else if (a.y >= a.z) { face = select(3, 2, v.y >= 0.0); }
  else { face = select(5, 4, v.z >= 0.0); }
  let lc = (g.point_vp[face] * vec4<f32>(wpos, 1.0));
  let ndc = (lc.xyz / lc.w);
  let uv = vec2<f32>(((ndc.x * 0.5) + 0.5), (0.5 - (ndc.y * 0.5)));
  if (((uv.x < 0.0) || (uv.x > 1.0) || (uv.y < 0.0) || (uv.y > 1.0) || (ndc.z > 1.0))) { return 1.0; }
  let stored = textureSampleLevel(pointArr, pointSamp, uv, face, 0.0).r;
  let bias = 0.015;
  if ((cur - bias) > stored) { return 0.0; }
  return 1.0;
}

// PCF 3×3 shadow lookup against atlas `layer` (directional ortho or spot perspective).
fn shadow(layer: i32, wpos: vec3<f32>, ndl: f32) -> f32 {
  let lc = (g.shadow_vp[layer] * vec4<f32>(wpos, 1.0));
  let ndc = (lc.xyz / lc.w);
  let uv = vec2<f32>(((ndc.x * 0.5) + 0.5), (0.5 - (ndc.y * 0.5)));
  if (((uv.x < 0.0) || (uv.x > 1.0) || (uv.y < 0.0) || (uv.y > 1.0) || (ndc.z > 1.0) || (ndc.z < 0.0))) {
    return 1.0;
  }
  let bias = max((g.tune.z * (1.0 - ndl)), (g.tune.z * 0.25));
  let texel = g.tune.w;
  var lit = 0.0;
  for (var dx = -1; (dx <= 1); dx++) {
    for (var dy = -1; (dy <= 1); dy++) {
      lit += textureSampleCompareLevel(shadowMap, shadowSamp, (uv + (vec2<f32>(f32(dx), f32(dy)) * texel)), layer, (ndc.z - bias));
    }
  }
  return (lit / 9.0);
}

// Procedural environment radiance in a world direction — a cheap stand-in for a
// prefiltered env map: a ground→sky gradient. Used for both the IBL irradiance
// (sampled along N) and the IBL specular (sampled along the reflection vector).
fn env_radiance(dir: vec3<f32>) -> vec3<f32> {
  let t = smoothstep(-0.25, 0.45, dir.y);
  return mix(g.ground.rgb, g.sky.rgb, t);
}

struct VO {
  @builtin(position) clip: vec4<f32>,
  @location(0) n: vec3<f32>,
  @location(1) col: vec3<f32>,
  @location(2) wpos: vec3<f32>,
  @location(3) mat: vec3<f32>,
  @location(4) alpha: f32,
};

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
      @location(2) m0: vec4<f32>, @location(3) m1: vec4<f32>, @location(4) m2: vec4<f32>, @location(5) m3: vec4<f32>,
      @location(6) color: vec4<f32>, @location(7) material: vec4<f32>) -> VO {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  let world = (model * vec4<f32>(pos, 1.0));
  var o: VO;
  o.clip = (g.vp * world);
  o.n = normalize(((model * vec4<f32>(normal, 0.0))).xyz);
  o.col = color.rgb;
  o.wpos = world.xyz;
  o.mat = material.xyz;
  o.alpha = color.a;  // < 1.0 → alpha-blended instance
  return o;
}

// One light's diffuse+spec contribution (Blinn-Phong, energy-shaped for metals),
// shadowed by its own atlas layer when it casts.
fn light_contrib(idx: u32, N: vec3<f32>, V: vec3<f32>, wpos: vec3<f32>,
                 albedo: vec3<f32>, metallic: f32, shininess: f32) -> vec3<f32> {
  let lt = g.lights[idx];
  let kind = lt.dir.w;
  var L: vec3<f32>;
  var atten = 1.0;
  if (kind < 0.5) {
    // directional: dir points light→surface, so incoming L = -dir
    L = normalize(-lt.dir.xyz);
  } else {
    let d = (lt.pos.xyz - wpos);
    let dist = length(d);
    L = (d / max(dist, 0.0001));
    let range = max(lt.pos.w, 0.0001);
    let f = clamp(1.0 - (dist / range), 0.0, 1.0);
    atten = (f * f);
    if (kind > 1.5) {
      // spot cone: angle between spot axis (dir) and light→fragment (-L)
      let cd = dot(normalize(lt.dir.xyz), -L);
      atten = (atten * smoothstep(lt.spot.y, lt.spot.x, cd));
    }
  }
  let ndl = max(dot(N, L), 0.0);
  let H = normalize((L + V));
  let spec = (pow(max(dot(N, H), 0.0), shininess) * g.tune.x);
  let specTint = mix(vec3<f32>(1.0), albedo, metallic);
  let radiance = (lt.color.rgb * lt.color.w * atten);
  let layer = i32(lt.spot.w);
  var sh = 1.0;
  if (layer == POINT_LAYER) { sh = point_shadow(wpos, lt.pos.xyz, lt.pos.w); }
  else if (layer >= 0) { sh = shadow(layer, wpos, ndl); }
  return (radiance * sh * ((albedo * ndl * (1.0 - (metallic * 0.85))) + (specTint * spec)));
}

@fragment
fn fs(i: VO) -> @location(0) vec4<f32> {
  let N = normalize(i.n);
  let V = normalize((g.eye.xyz - i.wpos));
  let metallic = clamp(i.mat.x, 0.0, 1.0);
  let rough = clamp(i.mat.y, 0.04, 1.0);
  let emissive = i.mat.z;
  let shininess = g.tune.y * (1.0 - rough) + 4.0;

  // IBL: image-based when an env map is uploaded (sky.w), else procedural gradient.
  let R = reflect(-V, N);
  let fres = pow(1.0 - max(dot(N, V), 0.0), 5.0);
  var irradiance: vec3<f32>;
  var ibl_spec: vec3<f32>;
  if (g.sky.w > 0.5) {
    let maxlod = f32(textureNumLevels(envTex)) - 1.0;
    irradiance = g.amb.rgb + env_sample(N, max(maxlod - 1.0, 0.0)) * g.amb.w; // blurred top mip = diffuse
    ibl_spec = env_sample(R, rough * maxlod) * g.amb.w * mix(0.04 + fres, 1.0, metallic);
  } else {
    irradiance = g.amb.rgb + env_radiance(N) * g.ground.w;
    ibl_spec = env_radiance(R) * g.amb.w * mix(0.04 + fres, 1.0, metallic);
  }

  var c = (i.col * irradiance) + (i.col * ibl_spec * (0.10 + metallic * 0.90));

  let n = u32(g.eye.w);
  for (var k = 0u; (k < n) && (k < MAX_LIGHTS); k++) {
    c += light_contrib(k, N, V, i.wpos, i.col, metallic, shininess);
  }
  c += (i.col * emissive);
  return vec4<f32>(c, i.alpha);  // linear HDR (alpha used by the blend pipeline); post.wgsl tonemaps
}
