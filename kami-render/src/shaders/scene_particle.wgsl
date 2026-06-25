struct U { view_proj: mat4x4<f32>, cam_right: vec3<f32>, _p0: f32, cam_up: vec3<f32>, _p1: f32 };
@group(0) @binding(0) var<uniform> u: U;
struct VsIn { @location(0) quad: vec2<f32>, @location(1) ipos: vec3<f32>, @location(2) icol: vec3<f32>, @location(3) isize: f32, @location(4) iage: f32, @location(5) ilife: f32 };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) color: vec3<f32>, @location(1) alpha: f32, @location(2) uv: vec2<f32> };
@vertex
fn vs(i: VsIn) -> VsOut {
  let world = (i.ipos + (u.cam_right * (i.quad.x * i.isize)) + (u.cam_up * (i.quad.y * i.isize)));
  var o: VsOut;
  o.clip = (u.view_proj * vec4<f32>(world, 1.0));
  o.color = i.icol;
  o.alpha = clamp((1.0 - (i.iage / max(i.ilife, 0.001))), 0.0, 1.0);
  o.uv = i.quad;
  return o;
}
@fragment
fn fs(i: VsOut) -> @location(0) vec4<f32> {
  let d = (length(i.uv) * 2.0);
  let edge = (1.0 - smoothstep(0.7, 1.0, d));
  let a = (i.alpha * edge);
  if ((a <= 0.0)) {
    discard;
  }
  return vec4<f32>(i.color, a);
}
