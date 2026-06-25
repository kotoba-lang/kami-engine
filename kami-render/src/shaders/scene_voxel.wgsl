struct U { view_proj: mat4x4<f32>, cam_pos: vec3<f32>, _p0: f32, sun_dir: vec3<f32>, _p1: f32, sun_color: vec3<f32>, fog_density: f32, fog_color: vec3<f32>, _p2: f32 };
@group(0) @binding(0) var<uniform> u: U;
struct VsIn { @location(0) pos: vec3<f32>, @location(1) norm: vec3<f32>, @location(2) col: vec3<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) world: vec3<f32>, @location(1) norm: vec3<f32>, @location(2) col: vec3<f32> };
@vertex
fn vs(i: VsIn) -> VsOut {
  var o: VsOut;
  o.clip = (u.view_proj * vec4<f32>(i.pos, 1.0));
  o.world = i.pos;
  o.norm = i.norm;
  o.col = i.col;
  return o;
}
@fragment
fn fs(i: VsOut) -> @location(0) vec4<f32> {
  let n = normalize(i.norm);
  let sun = normalize(u.sun_dir);
  let ndotl = max(((dot(n, sun) * 0.5) + 0.5), 0.0);
  let ambient = 0.35;
  let diffuse = ((ndotl * (1.0 - ambient)) + ambient);
  let lit = (i.col * diffuse * u.sun_color);
  let dist = length((u.cam_pos - i.world));
  let fog_t = (1.0 - exp(((-dist) * u.fog_density)));
  let color = mix(lit, u.fog_color, clamp(fog_t, 0.0, 0.6));
  return vec4<f32>(color, 1.0);
}
