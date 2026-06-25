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
@group(3) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;
struct MorphInfo { target_count: u32, vertex_count: u32, _pad0: u32, _pad1: u32, weights: array<vec4<f32>, 16> };
@group(4) @binding(0) var<storage, read> morph_deltas: array<f32>;
@group(4) @binding(1) var<uniform> morph_info: MorphInfo;
struct VertexInput { @builtin(vertex_index) vid: u32, @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(8) joints: vec4<u32>, @location(9) weights: vec4<f32> };
struct InstanceInput { @location(3) model_0: vec4<f32>, @location(4) model_1: vec4<f32>, @location(5) model_2: vec4<f32>, @location(6) model_3: vec4<f32> };
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) world_position: vec3<f32>, @location(1) world_normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) shadow_coord: vec4<f32> };
fn skin_matrix(joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
  let w_sum = (weights.x + weights.y + weights.z + weights.w);
  // Guard against unskinned verts (all weights 0) → return identity.;
  if ((w_sum < 0.0001)) {
    return mat4x4<f32>(vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, 1.0, 0.0, 0.0), vec4<f32>(0.0, 0.0, 1.0, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0), );
  }
  let inv = (1.0 / w_sum);
  let m0 = (bones[joints.x] * (weights.x * inv));
  let m1 = (bones[joints.y] * (weights.y * inv));
  let m2 = (bones[joints.z] * (weights.z * inv));
  let m3 = (bones[joints.w] * (weights.w * inv));
  return mat4x4<f32>((m0[0] + m1[0] + m2[0] + m3[0]), (m0[1] + m1[1] + m2[1] + m3[1]), (m0[2] + m1[2] + m2[2] + m3[2]), (m0[3] + m1[3] + m2[3] + m3[3]), );
}
@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
  // Morph blend (GPU). Skipped when target_count == 0.;
  var morphed_pos = vertex.position;
  let tc = morph_info.target_count;
  let vc = morph_info.vertex_count;
  for (var t: u32 = 0u; (t < tc); t = t + 1u) {
    let w = morph_info.weights[t / 4u][t % 4u];
    if ((abs(w) > 0.001)) {
      let off = (t * vc + vertex.vid) * 3u;
      let dx = morph_deltas[off];
      let dy = morph_deltas[off + 1u];
      let dz = morph_deltas[off + 2u];
      morphed_pos = (morphed_pos + (vec3<f32>(dx, dy, dz) * w));
    }
  }
  let skin = skin_matrix(vertex.joints, vertex.weights);
  let model = mat4x4<f32>(instance.model_0, instance.model_1, instance.model_2, instance.model_3);
  let skinned_pos = (skin * vec4<f32>(morphed_pos, 1.0));
  let world_pos = (model * skinned_pos);
  let skin3 = mat3x3<f32>(skin[0].xyz, skin[1].xyz, skin[2].xyz);
  let normal_matrix = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
  var out: VertexOutput;
  out.clip_position = (camera.projection * camera.view * world_pos);
  out.world_position = world_pos.xyz;
  out.world_normal = normalize((normal_matrix * (skin3 * vertex.normal)));
  out.uv = vertex.uv;
  out.shadow_coord = (light.view_proj * world_pos);
  return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  let n = normalize(in.world_normal);
  let v = normalize((camera.position - in.world_position));
  let l = normalize((-light.direction));
  var albedo: vec3<f32>;
  if ((material.has_albedo_tex == 1u)) {
    albedo = ((textureSample(albedo_texture, albedo_sampler, in.uv)).rgb * material.albedo.rgb);
  } else {
    albedo = material.albedo.rgb;
  }
  let shade_color = material.subsurface_color.rgb;
  let shade_shift = material.subsurface_color.a;
  let shade_toony = material.sss_r0;
  let ndl = dot(n, l);
  let shade_width = max(0.01, (1.0 - shade_toony));
  let shade_t = smoothstep((shade_shift - shade_width), (shade_shift + shade_width), ndl);
  let lit_color = (albedo * light.color * light.intensity);
  let shade_result = (albedo * shade_color * light.color * light.intensity * 0.8);
  let shaded = mix(shade_result, lit_color, shade_t);
  let rim_intensity = material.sss_r1;
  let rim_fresnel = material.sss_r2;
  let rim_color = material.hair_scatter.rgb;
  let rim_lift = material.hair_scatter.a;
  let rim_dot = (1.0 - max(dot(n, v), 0.0));
  let rim = (pow(rim_dot, max(rim_fresnel, 0.01)) * rim_intensity);
  let rim_contribution = (rim_color * max((rim - rim_lift), 0.0));
  let sky = vec3<f32>(0.25, 0.28, 0.35);
  let ground = vec3<f32>(0.15, 0.13, 0.12);
  let ambient = (mix(ground, sky, ((n.y * 0.5) + 0.5)) * albedo);
  let emission = vec3<f32>(material.emission_r, material.emission_g, material.emission_b);
  var color = (shaded + ambient + rim_contribution + emission);
  let gamma = pow(color, vec3<f32>((1.0 / 2.2)));
  return vec4<f32>(gamma, material.albedo.a);
}
