// point_shadow.wgsl — omnidirectional point-light shadow (ADR-0044 phase 6).
// One invocation per cube face renders the instances from the light's POV (90° fov)
// and stores linear distance (frag→light)/range into an R16Float cube face. The lit
// pass samples it by world-space direction (`point_shadow` in lit_ir.wgsl).
struct PS { vp: mat4x4<f32>, light: vec4<f32> };  // light = xyz pos, w range
@group(0) @binding(0) var<uniform> ps: PS;

struct VO { @builtin(position) clip: vec4<f32>, @location(0) world: vec3<f32> };

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
      @location(2) m0: vec4<f32>, @location(3) m1: vec4<f32>, @location(4) m2: vec4<f32>, @location(5) m3: vec4<f32>,
      @location(6) color: vec4<f32>, @location(7) material: vec4<f32>) -> VO {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  let world = (model * vec4<f32>(pos, 1.0));
  var o: VO;
  o.clip = (ps.vp * world);
  o.world = world.xyz;
  return o;
}

@fragment
fn fs(i: VO) -> @location(0) vec4<f32> {
  let d = (length(i.world - ps.light.xyz) / max(ps.light.w, 0.0001));
  return vec4<f32>(d, 0.0, 0.0, 1.0);
}
