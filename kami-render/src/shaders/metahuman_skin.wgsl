// MetaHuman Skin Shader — GPU skinning + dual-lobe SSS + detail normal.
//
// Vertex: skeletal animation (4-bone LBS) + morph target displacement.
// Fragment: Burley normalized diffusion SSS + detail normal blend + clearcoat.

// --- Bind Group 0: Camera + Lights ---
struct CameraUniform { view_proj: mat4x4<f32>, view: mat4x4<f32>, camera_pos: vec3<f32>, _pad0: f32 };
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct Light { direction: vec3<f32>, intensity: f32, color: vec3<f32>, _pad: f32 };
@group(0) @binding(1) var<uniform> light: Light;

// --- Bind Group 1: Model + Skin Params ---
struct ModelUniform { model: mat4x4<f32>, normal_matrix: mat4x4<f32> };
@group(1) @binding(0) var<uniform> model: ModelUniform;

struct SkinParams { base_color: vec3<f32>, roughness: f32, epidermis_thickness: f32, melanin_density: f32, dermis_thickness: f32, hemoglobin_density: f32, subdermal_scatter: f32, pore_density: f32, oiliness: f32, _pad: f32 };
@group(1) @binding(1) var<uniform> skin: SkinParams;

// --- Bind Group 2: Joint Matrices (GPU Skinning) ---
@group(2) @binding(0) var<storage, read> joint_matrices: array<mat4x4<f32>>;
// Morph target deltas (position + normal per target, flattened)
@group(2) @binding(1) var<storage, read> morph_deltas: array<vec4<f32>>;

struct MorphWeights { weights: array<vec4<f32>, 16>, active_count: u32, _pad0: f32, _pad1: f32, _pad2: f32 };
@group(2) @binding(2) var<uniform> morph_weights: MorphWeights;

// --- Bind Group 3: Textures ---
@group(3) @binding(0) var t_diffuse: texture_2d<f32>;
@group(3) @binding(1) var s_diffuse: sampler;
@group(3) @binding(2) var t_normal: texture_2d<f32>;
@group(3) @binding(3) var s_normal: sampler;
@group(3) @binding(4) var t_roughness: texture_2d<f32>;
@group(3) @binding(5) var s_roughness: sampler;

// --- Vertex Input ---
struct VertexInput { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) tangent: vec4<f32>, @location(4) joint_indices: vec4<u32>, @location(5) joint_weights: vec4<f32> };

struct VertexOutput { @builtin(position) clip_pos: vec4<f32>, @location(0) world_pos: vec3<f32>, @location(1) world_normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) world_tangent: vec3<f32>, @location(4) bitangent_sign: f32 };

// --- Vertex Shader: 4-bone LBS skinning ---
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
  // Linear Blend Skinning (4 influences);
  var skin_mat = mat4x4<f32>(
          vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0)
      );
  let w = in.joint_weights;
  let j = in.joint_indices;
  skin_mat += joint_matrices[j.x] * w.x;
  skin_mat += joint_matrices[j.y] * w.y;
  skin_mat += joint_matrices[j.z] * w.z;
  skin_mat += joint_matrices[j.w] * w.w;
  let skinned_pos = (skin_mat * vec4<f32>(in.position, 1.0));
  let skinned_normal = normalize(((skin_mat * vec4<f32>(in.normal, 0.0))).xyz);
  let skinned_tangent = normalize(((skin_mat * vec4<f32>(in.tangent.xyz, 0.0))).xyz);
  let world_pos = ((model.model * skinned_pos)).xyz;
  let world_normal = normalize(((model.normal_matrix * vec4<f32>(skinned_normal, 0.0))).xyz);
  let world_tangent = normalize(((model.normal_matrix * vec4<f32>(skinned_tangent, 0.0))).xyz);
  var out: VertexOutput;
  out.clip_pos = (camera.view_proj * vec4<f32>(world_pos, 1.0));
  out.world_pos = world_pos;
  out.world_normal = world_normal;
  out.uv = in.uv;
  out.world_tangent = world_tangent;
  out.bitangent_sign = in.tangent.w;
  return out;
}

// --- Fragment Shader: Dual-lobe SSS + Detail Normal ---

/// Burley normalized diffusion profile approximation.
/// Models light scattering through skin layers.
fn burley_sss(d: f32, scatter_radius: f32) -> f32 {
  let s = (1.0 / max(scatter_radius, 0.001));
  let r = (d * s);
  return ((s * (exp((-r)) + exp(((-r) / 3.0)))) / (8.0 * 3.14159));
}

/// Fresnel-Schlick approximation.
fn fresnel_schlick(cos_theta: f32, f0: f32) -> f32 {
  return (f0 + ((1.0 - f0) * pow((1.0 - cos_theta), 5.0)));
}

/// GGX normal distribution function.
fn ggx_ndf(n_dot_h: f32, roughness: f32) -> f32 {
  let a = (roughness * roughness);
  let a2 = (a * a);
  let denom = ((n_dot_h * n_dot_h * (a2 - 1.0)) + 1.0);
  return (a2 / (3.14159 * denom * denom));
}

/// Smith-GGX geometry function.
fn smith_ggx(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
  let r = (roughness + 1.0);
  let k = ((r * r) / 8.0);
  let g1 = (n_dot_v / ((n_dot_v * (1.0 - k)) + k));
  let g2 = (n_dot_l / ((n_dot_l * (1.0 - k)) + k));
  return (g1 * g2);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  // Sample textures;
  let diffuse_tex = (textureSample(t_diffuse, s_diffuse, in.uv)).rgb;
  let normal_tex = (((textureSample(t_normal, s_normal, in.uv)).rgb * 2.0) - 1.0);
  let roughness_tex = (textureSample(t_roughness, s_roughness, in.uv)).r;
  // TBN matrix for normal mapping;
  let N = normalize(in.world_normal);
  let T = normalize(in.world_tangent);
  let B = (cross(N, T) * in.bitangent_sign);
  let tbn = mat3x3<f32>(T, B, N);
  let mapped_normal = normalize((tbn * normal_tex));
  let V = normalize((camera.camera_pos - in.world_pos));
  let L = normalize((-light.direction));
  let H = normalize((V + L));
  let n_dot_l = max(dot(mapped_normal, L), 0.0);
  let n_dot_v = max(dot(mapped_normal, V), 0.001);
  let n_dot_h = max(dot(mapped_normal, H), 0.0);
  let v_dot_h = max(dot(V, H), 0.0);
  // Base color: texture × skin params;
  let base = (skin.base_color * diffuse_tex);
  let roughness = (skin.roughness * roughness_tex);
  // --- SSS Diffuse (dual-lobe Burley) ---;
  // Lobe 1: epidermis (melanin absorption, short scatter);
  let epidermis_scatter = (0.005 * skin.epidermis_thickness);
  let epidermis_color = vec3<f32>((1.0 - (skin.melanin_density * 0.6)), (1.0 - (skin.melanin_density * 0.3)), (1.0 - (skin.melanin_density * 0.1)));
  let sss_epid = (burley_sss(0.01, epidermis_scatter) * epidermis_color);
  // Lobe 2: dermis (hemoglobin scattering, longer range);
  let dermis_scatter = (0.02 * skin.dermis_thickness);
  let dermis_color = vec3<f32>((0.9 + (skin.hemoglobin_density * 0.1)), (0.4 + (skin.hemoglobin_density * 0.15)), 0.3);
  let sss_derm = (burley_sss(0.01, dermis_scatter) * dermis_color);
  // Combined SSS diffuse;
  let sss_factor = skin.subdermal_scatter;
  let diffuse_sss = ((base * n_dot_l * (1.0 - sss_factor)) + (base * (sss_epid + sss_derm) * sss_factor));
  // --- Specular (GGX BRDF) ---;
  let f0 = 0.028;
  // skin Fresnel reflectance at normal incidence;
  let F = fresnel_schlick(v_dot_h, f0);
  let D = ggx_ndf(n_dot_h, roughness);
  let G = smith_ggx(n_dot_v, n_dot_l, roughness);
  let specular = ((D * G * F) / max((4.0 * n_dot_v * n_dot_l), 0.001));
  // --- Clearcoat (oiliness) ---;
  let clearcoat_rough = 0.1;
  let cc_D = ggx_ndf(n_dot_h, clearcoat_rough);
  let cc_F = fresnel_schlick(v_dot_h, 0.04);
  let cc_G = smith_ggx(n_dot_v, n_dot_l, clearcoat_rough);
  let clearcoat = ((skin.oiliness * 0.3 * (cc_D * cc_G * cc_F)) / max((4.0 * n_dot_v * n_dot_l), 0.001));
  // Ambient;
  let ambient = (base * 0.08);
  // Final;
  let color = (ambient + ((diffuse_sss + specular + clearcoat) * light.color * light.intensity));
  // Tone mapping (ACES filmic);
  let aces = ((color * ((2.51 * color) + 0.03)) / ((color * ((2.43 * color) + 0.59)) + 0.14));
  return vec4<f32>(pow(aces, vec3<f32>((1.0 / 2.2))), 1.0);
}