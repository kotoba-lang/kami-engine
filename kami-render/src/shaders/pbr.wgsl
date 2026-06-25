// KAMI Engine — PBR shader
struct CameraUniform { view: mat4x4<f32>, projection: mat4x4<f32>, position: vec3<f32>, _pad: f32 };
struct LightUniform { direction: vec3<f32>, _pad0: f32, color: vec3<f32>, intensity: f32, view_proj: mat4x4<f32> };
struct MaterialUniform { albedo: vec4<f32>, metallic: f32, roughness: f32, has_albedo_tex: u32, has_normal_tex: u32, subsurface_color: vec4<f32>, sss_r0: f32, sss_r1: f32, sss_r2: f32, sss_model: u32, aniso_t0: f32, aniso_t1: f32, aniso_t2: f32, aniso_strength: f32, hair_scatter: vec4<f32>, clearcoat: f32, clearcoat_roughness: f32, emission_r: f32, emission_g: f32, emission_b: f32, tex_flags: u32, parallax_depth: f32, _pad_end: f32 };
@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> light: LightUniform;
@group(1) @binding(0) var<uniform> material: MaterialUniform;
@group(1) @binding(1) var albedo_texture: texture_2d<f32>;
@group(1) @binding(2) var albedo_sampler: sampler;
@group(1) @binding(3) var normal_texture: texture_2d<f32>;
@group(1) @binding(4) var normal_sampler: sampler;
@group(1) @binding(5) var mr_texture: texture_2d<f32>;
@group(1) @binding(6) var mr_sampler: sampler;
@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_sampler: sampler_comparison;
struct VertexInput { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32> };
struct InstanceInput { @location(3) model_0: vec4<f32>, @location(4) model_1: vec4<f32>, @location(5) model_2: vec4<f32>, @location(6) model_3: vec4<f32> };
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) world_position: vec3<f32>, @location(1) world_normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) shadow_coord: vec4<f32> };
@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
  let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3,);
  let world_pos = (model * vec4<f32>(vertex.position, 1.0));
  let normal_matrix = mat3x3<f32>((model[0]).xyz, (model[1]).xyz, (model[2]).xyz);
  var out: VertexOutput;
  out.clip_position = (camera.projection * camera.view * world_pos);
  out.world_position = world_pos.xyz;
  out.world_normal = normalize((normal_matrix * vertex.normal));
  out.uv = vertex.uv;
  out.shadow_coord = (light.view_proj * world_pos);
  return out;
}
const PI: f32 = 3.14159265359;
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
  let a = (roughness * roughness);
  let a2 = (a * a);
  let denom = ((n_dot_h * n_dot_h * (a2 - 1.0)) + 1.0);
  return (a2 / (PI * denom * denom));
}
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
  return (f0 + ((1.0 - f0) * pow((1.0 - cos_theta), 5.0)));
}
fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
  let r = (roughness + 1.0);
  let k = ((r * r) / 8.0);
  let ggx1 = (n_dot_v / ((n_dot_v * (1.0 - k)) + k));
  let ggx2 = (n_dot_l / ((n_dot_l * (1.0 - k)) + k));
  return (ggx1 * ggx2);
}
fn shadow_factor(shadow_coord: vec4<f32>) -> f32 {
  let proj = (shadow_coord.xyz / shadow_coord.w);
  let uv = ((proj.xy * 0.5) + 0.5);
  let flip_uv = vec2<f32>(uv.x, (1.0 - uv.y));
  let depth = (proj.z - 0.005);
  let clamped_uv = clamp(flip_uv, vec2<f32>(0.001), vec2<f32>(0.999));
  let shadow = textureSampleCompare(shadow_map, shadow_sampler, clamped_uv, depth);
  let in_bounds = (step(0.0, flip_uv.x) * step(flip_uv.x, 1.0) * step(0.0, flip_uv.y) * step(flip_uv.y, 1.0));
  return mix(1.0, shadow, in_bounds);
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  var albedo: vec3<f32>;
  if ((material.has_albedo_tex == 1u)) {
    albedo = (textureSample(albedo_texture, albedo_sampler, in.uv)).rgb;
  } else {
    albedo = material.albedo.rgb;
  }
  if ((material.parallax_depth > 99.0)) {
    return vec4<f32>(albedo, 1.0);
  }
  let n = normalize(in.world_normal);
  let v = normalize((camera.position - in.world_position));
  let l = normalize((-light.direction));
  let h = normalize((v + l));
  let n_dot_l = max(dot(n, l), 0.0);
  let n_dot_v = max(dot(n, v), 0.001);
  let n_dot_h = max(dot(n, h), 0.0);
  let h_dot_v = max(dot(h, v), 0.0);
  var metallic: f32;
  var roughness: f32;
  let has_mr = (material.tex_flags & 1u) != 0u;
  if (has_mr) {
    let mr = textureSample(mr_texture, mr_sampler, in.uv);
    metallic = mr.b;
    roughness = mr.g;
  } else {
    metallic = material.metallic;
    roughness = material.roughness;
  }
  let f0 = mix(vec3<f32>(0.04), albedo, metallic);
  let d = distribution_ggx(n_dot_h, roughness);
  let f = fresnel_schlick(h_dot_v, f0);
  let g = geometry_smith(n_dot_v, n_dot_l, roughness);
  let specular = ((d * f * g) / ((4.0 * n_dot_v * n_dot_l) + 0.0001));
  let k_s = f;
  let k_d = ((1.0 - k_s) * (1.0 - metallic));
  let diffuse = ((k_d * albedo) / PI);
  let shadow = shadow_factor(in.shadow_coord);
  let radiance = (light.color * light.intensity);
  let lo_key = ((diffuse + specular) * radiance * n_dot_l * shadow);
  let fill_dir = normalize(vec3<f32>(0.7, (-0.2), 0.9));
  let fill_ndl = max(dot(n, fill_dir), 0.0);
  let fill_color = vec3<f32>(0.4, 0.38, 0.5);
  let lo_fill = (albedo * fill_color * fill_ndl * 0.3);
  let back_dir = normalize(vec3<f32>(0.0, 0.8, (-0.6)));
  let back_ndl = max(dot(n, back_dir), 0.0);
  let lo_back = (albedo * vec3<f32>(0.2, 0.2, 0.3) * back_ndl * 0.3);
  let rim_dot = (1.0 - max(dot(n, v), 0.0));
  let rim = (pow(rim_dot, 2.5) * 0.6);
  let lo_rim = (vec3<f32>(0.7, 0.8, 1.0) * rim);
  let sky_color = vec3<f32>(0.15, 0.18, 0.25);
  let ground_color = vec3<f32>(0.08, 0.06, 0.04);
  let hemisphere_t = ((n.y * 0.5) + 0.5);
  let ambient = (mix(ground_color, sky_color, hemisphere_t) * albedo);
  var color = (ambient + lo_key + lo_fill + lo_back + lo_rim);
  if ((material.sss_model > 0u)) {
    let sss_strength = material.subsurface_color.a;
    let wrap_ndl = (max((dot(n, l) + 0.3), 0.0) / 1.3);
    let sss_contrib = (material.subsurface_color.rgb * wrap_ndl * sss_strength * radiance);
    color = (color + sss_contrib);
  }
  color = (color + vec3<f32>(material.emission_r, material.emission_g, material.emission_b));
  let mapped = (color / (color + vec3<f32>(1.0)));
  let gamma = pow(mapped, vec3<f32>((1.0 / 2.2)));
  return vec4<f32>(gamma, material.albedo.a);
}
struct VertexInputColor { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(7) color: vec4<f32> };
struct VertexOutputColor { @builtin(position) clip_position: vec4<f32>, @location(0) world_position: vec3<f32>, @location(1) world_normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) shadow_coord: vec4<f32>, @location(4) vertex_color: vec4<f32> };
@vertex
fn vs_color(vertex: VertexInputColor, instance: InstanceInput) -> VertexOutputColor {
  let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3,);
  let world_pos = (model * vec4<f32>(vertex.position, 1.0));
  let normal_matrix = mat3x3<f32>((model[0]).xyz, (model[1]).xyz, (model[2]).xyz);
  var out: VertexOutputColor;
  out.clip_position = (camera.projection * camera.view * world_pos);
  out.world_position = world_pos.xyz;
  out.world_normal = normalize((normal_matrix * vertex.normal));
  out.uv = vertex.uv;
  out.shadow_coord = (light.view_proj * world_pos);
  out.vertex_color = vertex.color;
  return out;
}
@fragment
fn fs_color(in: VertexOutputColor) -> @location(0) vec4<f32> {
  let albedo = in.vertex_color.rgb;
  let n = normalize(in.world_normal);
  let l = normalize((-light.direction));
  let n_dot_l = max(dot(n, l), 0.0);
  let shadow = shadow_factor(in.shadow_coord);
  let radiance = (vec3<f32>(1.0, 1.0, 1.0) * light.intensity * 0.5);
  let lo_key = (albedo * radiance * n_dot_l * shadow);
  let fill_dir = normalize(vec3<f32>(0.3, 0.8, 0.5));
  let fill_ndl = max(dot(n, fill_dir), 0.0);
  let lo_fill = (albedo * vec3<f32>(0.6, 0.6, 0.65) * fill_ndl * 0.35);
  let sky_amb = vec3<f32>(0.40, 0.42, 0.50);
  let gnd_amb = vec3<f32>(0.25, 0.22, 0.20);
  let hem_t = ((n.y * 0.5) + 0.5);
  let ambient = (mix(gnd_amb, sky_amb, hem_t) * albedo);
  let ao = (0.85 + (0.15 * max(n.y, 0.0)));
  var color = ((ambient + lo_key + lo_fill) * ao);
  let dist = length((camera.position - in.world_position));
  let fog_start = 48.0;
  let fog_end = 128.0;
  let fog_factor = clamp(((dist - fog_start) / (fog_end - fog_start)), 0.0, 0.6);
  let fog_color = vec3<f32>(0.53, 0.65, 0.75);
  color = mix(color, fog_color, fog_factor);
  let gamma = pow(color, vec3<f32>((1.0 / 2.2)));
  return vec4<f32>(gamma, 1.0);
}
@vertex
fn vs_shadow(vertex: VertexInput, instance: InstanceInput) -> @builtin(position) vec4<f32> {
  let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3,);
  return (light.view_proj * model * vec4<f32>(vertex.position, 1.0));
}
