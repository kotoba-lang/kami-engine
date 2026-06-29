// shadow_ir.wgsl — depth-only shadow pass for the render-IR atlas (ADR-0044 phase 6).
// One invocation per shadow-casting light renders the instances from that light's POV
// (directional = ortho `light_vp`, spot = perspective `light_vp`) into one atlas layer.
struct S { light_vp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> s: S;

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
      @location(2) m0: vec4<f32>, @location(3) m1: vec4<f32>, @location(4) m2: vec4<f32>, @location(5) m3: vec4<f32>,
      @location(6) color: vec4<f32>, @location(7) material: vec4<f32>) -> @builtin(position) vec4<f32> {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  return (s.light_vp * model * vec4<f32>(pos, 1.0));
}
