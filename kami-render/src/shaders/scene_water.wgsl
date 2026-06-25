struct U { view_proj: mat4x4<f32>, cam_pos: vec3<f32>, time: f32, sun_dir: vec3<f32>, water_y: f32, fog_color: vec3<f32>, _p0: f32, base_col: vec3<f32>, _p1: f32 };
@group(0) @binding(0) var<uniform> u: U;
struct VsIn { @location(0) pos: vec3<f32>, @location(1) uv: vec2<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) world: vec3<f32>, @location(1) uv: vec2<f32> };
@vertex
fn vs(i: VsIn) -> VsOut {
  var o: VsOut;
  let k1 = 0.05;
  let k2 = 0.07;
  let a = 0.15;
  let dy = (a * (sin(((i.pos.x * k1) + (u.time * 1.3))) + sin(((i.pos.z * k2) + (u.time * 0.9)))));
  let world = vec3<f32>(i.pos.x, (u.water_y + dy), i.pos.z);
  o.clip = (u.view_proj * vec4<f32>(world, 1.0));
  o.world = world;
  o.uv = i.uv;
  return o;
}
@fragment
fn fs(i: VsOut) -> @location(0) vec4<f32> {
  let view = normalize((u.cam_pos - i.world));
  let up = vec3<f32>(0.0, 1.0, 0.0);
  let ndotv = clamp(dot(up, view), 0.0, 1.0);
  let fresnel = pow((1.0 - ndotv), 4.0);
  let half = normalize((u.sun_dir + view));
  let spec = pow(max(dot(up, half), 0.0), 64.0);
  let sun_bright = max(u.sun_dir.y, 0.0);
  let dist = length((u.cam_pos - i.world));
  let fog_t = (1.0 - exp(((-dist) * 0.0015)));
  let base = mix(u.base_col, u.fog_color, (fog_t * 0.6));
  let reflective = mix(base, u.fog_color, fresnel);
  let lit = (reflective + ((vec3<f32>(1.0, 0.95, 0.85) * spec) * sun_bright));
  return vec4<f32>(lit, 0.85);
}
