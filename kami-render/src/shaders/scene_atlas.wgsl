struct U { view_proj: mat4x4<f32>, cam_right: vec3<f32>, _p0: f32, cam_up: vec3<f32>, _p1: f32 };
@group(0) @binding(0) var<uniform> u: U;
struct VsIn { @location(0) quad: vec2<f32>, @location(1) ipos: vec3<f32>, @location(2) itint: vec3<f32>, @location(3) isize: f32, @location(4) islot: u32, @location(5) irot: f32, @location(6) ialpha: f32 };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) tint: vec3<f32>, @location(1) alpha: f32, @location(2) uv: vec2<f32>, @location(3) @interpolate(flat) slot: u32 };
@vertex
fn vs(i: VsIn) -> VsOut {
  let c = cos(i.irot);
  let s = sin(i.irot);
  let rq = vec2<f32>(((i.quad.x * c) - (i.quad.y * s)), ((i.quad.x * s) + (i.quad.y * c)));
  let world = (i.ipos + (u.cam_right * (rq.x * i.isize)) + (u.cam_up * (rq.y * i.isize)));
  var o: VsOut;
  o.clip = (u.view_proj * vec4<f32>(world, 1.0));
  o.tint = i.itint;
  o.alpha = i.ialpha;
  o.uv = (i.quad + vec2<f32>(0.5, 0.5));
  o.slot = i.islot;
  return o;
}
fn sample_shape(slot: u32, uv: vec2<f32>) -> vec2<f32> {
  let c = (uv - vec2<f32>(0.5, 0.5));
  let r = length(c);
  if (slot <= 2u) {
    let scale = (f32(slot) * 0.08);
    let dy = uv.y;
    let width = ((0.32 + scale) - (dy * (0.4 + scale)));
    let mask = (step(abs(c.x), width) * smoothstep((0.55 + (scale * 0.5)), 0.08, r));
    let shade = (mix(0.25, 1.0, (1.0 - uv.y)) + (0.1 * sin((uv.y * 9.0))));
    return vec2<f32>(mask, clamp(shade, 0.0, 1.2));
  }
  if (slot == 3u) {
    return vec2<f32>(smoothstep(0.28, 0.05, r), 1.3);
  }
  if (slot == 4u) {
    return vec2<f32>((smoothstep(0.42, 0.2, r) * 0.4), 0.85);
  }
  if (slot == 5u) {
    return vec2<f32>((smoothstep(0.46, 0.17, r) * 0.7), 0.7);
  }
  if (slot == 6u) {
    return vec2<f32>(smoothstep(0.34, 0.18, r), 0.55);
  }
  if (slot == 7u) {
    return vec2<f32>(smoothstep(0.2, 0.03, r), 0.7);
  }
  if (slot == 8u) {
    let dy = (1.0 - uv.y);
    let width = (0.32 - (dy * 0.4));
    let mask = (step(abs(c.x), width) * smoothstep(0.55, 0.08, r));
    let shade = mix(0.7, 1.0, uv.y);
    return vec2<f32>(mask, shade);
  }
  if (slot == 9u) {
    let ring = (smoothstep(0.33, 0.4, r) - smoothstep(0.44, 0.5, r));
    return vec2<f32>((ring * 1.5), 1.0);
  }
  if (slot == 10u) {
    return vec2<f32>((smoothstep(0.5, 0.13, r) * 0.75), 1.0);
  }
  if (slot == 11u) {
    let ring = (smoothstep(0.42, 0.4, r) - smoothstep(0.36, 0.32, r));
    return vec2<f32>((ring * 1.8), 1.1);
  }
  if (slot == 12u) {
    let a = abs(c);
    let axis = ((step(0.06, a.y) * step(a.x, 0.06)) + (step(0.06, a.x) * step(a.y, 0.06)));
    let core = smoothstep(0.1, 0.0, r);
    let arm = (smoothstep(0.42, 0.0, max(a.x, a.y)) * axis);
    return vec2<f32>(clamp((core + (arm * 0.8)), 0.0, 1.0), 1.2);
  }
  if (slot == 13u) {
    let ring = (smoothstep(0.4, 0.44, r) - smoothstep(0.47, 0.5, r));
    return vec2<f32>((ring * 2.2), 1.3);
  }
  if (slot == 14u) {
    let angle = atan2(c.y, c.x);
    let arc = step(0.3, sin(((angle * 1.5) + 1.0)));
    let ring = (smoothstep(0.33, 0.4, r) - smoothstep(0.45, 0.5, r));
    return vec2<f32>((ring * arc * 1.5), 1.0);
  }
  if (slot == 15u) {
    let tail = (smoothstep(0.5, 0.05, abs(c.y)) * smoothstep(0.5, 0.1, (abs(c.x) + 0.1)));
    return vec2<f32>(tail, 1.0);
  }
  return vec2<f32>(0.0, 0.0);
}
@fragment
fn fs(i: VsOut) -> @location(0) vec4<f32> {
  let s = sample_shape(i.slot, i.uv);
  let mask = s.x;
  if ((mask < 0.01)) {
    discard;
  }
  let col = (i.tint * s.y);
  return vec4<f32>(col, (mask * i.alpha));
}
