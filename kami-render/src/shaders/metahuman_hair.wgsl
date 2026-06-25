struct CameraUniform { view_proj: mat4x4<f32>, camera_pos: vec3<f32>, _pad0: f32 };
@group(0) @binding(0) var<uniform> camera: CameraUniform;
struct Light { direction: vec3<f32>, intensity: f32, color: vec3<f32>, _pad: f32 };
@group(0) @binding(1) var<uniform> light: Light;
struct HairParams { base_color: vec3<f32>, roughness: f32, highlight_color: vec3<f32>, shift_primary: f32, shift_secondary: f32, specular_power_r: f32, specular_power_trt: f32, specular_strength_r: f32, specular_strength_trt: f32, transmission: f32, ambient_occlusion_root: f32, _pad: f32 };
@group(1) @binding(0) var<uniform> hair: HairParams;
@group(1) @binding(1) var<uniform> model: mat4x4<f32>;
@group(2) @binding(0) var t_alpha: texture_2d<f32>;
@group(2) @binding(1) var s_alpha: sampler;
struct VertexInput { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32> };
struct VertexOutput { @builtin(position) clip_pos: vec4<f32>, @location(0) world_pos: vec3<f32>, @location(1) world_normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) tangent: vec3<f32> };
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
  let world_pos = ((model * vec4<f32>(in.position, 1.0))).xyz;
  let world_normal = normalize(((model * vec4<f32>(in.normal, 0.0))).xyz);
  let tangent = normalize(vec3<f32>(0.0, (-1.0), 0.0));
  var out: VertexOutput;
  out.clip_pos = (camera.view_proj * vec4<f32>(world_pos, 1.0));
  out.world_pos = world_pos;
  out.world_normal = world_normal;
  out.uv = in.uv;
  out.tangent = tangent;
  return out;
}
fn shift_tangent(T: vec3<f32>, N: vec3<f32>, shift: f32) -> vec3<f32> {
  return normalize((T + (N * shift)));
}
fn kajiya_diffuse(T: vec3<f32>, L: vec3<f32>) -> f32 {
  let TdotL = dot(T, L);
  return sqrt(max(0.0, (1.0 - (TdotL * TdotL))));
}
fn strand_specular(T: vec3<f32>, H: vec3<f32>, power: f32) -> f32 {
  let TdotH = dot(T, H);
  let sinTH = sqrt(max(0.0, (1.0 - (TdotH * TdotH))));
  return pow(sinTH, power);
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  let V = normalize((camera.camera_pos - in.world_pos));
  let L = normalize((-light.direction));
  let H = normalize((V + L));
  let N = normalize(in.world_normal);
  let T = normalize(in.tangent);
  let alpha_sample = (textureSample(t_alpha, s_alpha, in.uv)).a;
  if ((alpha_sample < 0.05)) {
    discard;
  }
  let root_tip = in.uv.y;
  let ao = mix(hair.ambient_occlusion_root, 1.0, sqrt(root_tip));
  let root_color = (hair.base_color * 0.6);
  let mid_color = hair.base_color;
  let tip_color = mix(hair.base_color, hair.highlight_color, 0.3);
  var strand_color: vec3<f32>;
  if ((root_tip < 0.3)) {
    strand_color = mix(root_color, mid_color, (root_tip / 0.3));
  } else {
    strand_color = mix(mid_color, tip_color, ((root_tip - 0.3) / 0.7));
  }
  let diffuse = kajiya_diffuse(T, L);
  let T1 = shift_tangent(T, N, (-hair.shift_primary));
  let spec_R = (strand_specular(T1, H, hair.specular_power_r) * hair.specular_strength_r);
  let T2 = shift_tangent(T, N, hair.shift_secondary);
  let spec_TRT = (strand_specular(T2, H, hair.specular_power_trt) * hair.specular_strength_trt);
  let back_lit = (max(0.0, dot((-V), L)) * hair.transmission);
  let tt_color = (strand_color * back_lit * 0.5);
  let ambient = (strand_color * 0.08);
  let color = (ambient + (((strand_color * diffuse * ao) + (vec3<f32>(1.0, 0.98, 0.95) * spec_R) + (hair.highlight_color * spec_TRT) + tt_color) * light.color * light.intensity));
  let tip_fade = mix(1.0, 0.0, pow((max((root_tip - 0.7), 0.0) / 0.3), 2.0));
  let final_alpha = (alpha_sample * tip_fade);
  let aces = ((color * ((2.51 * color) + 0.03)) / ((color * ((2.43 * color) + 0.59)) + 0.14));
  return vec4<f32>(pow(aces, vec3<f32>((1.0 / 2.2))), final_alpha);
}
