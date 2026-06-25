struct GaussianSplat {
    position: vec3<f32>,
    opacity: f32,
    scale: vec3<f32>,
    _pad0: f32,
    rotation: vec4<f32>,
    sh_dc: vec3<f32>,
    _pad1: f32,
}
struct SortEntry {
    distance: f32,
    index: u32,
}
struct SortParams {
    camera_pos: vec3<f32>,
    splat_count: u32,
}
@group(0) @binding(0) var<storage, read> splats: array<GaussianSplat>;
@group(0) @binding(1) var<storage, read_write> sort_entries: array<SortEntry>;
@group(0) @binding(2) var<uniform> sort_params: SortParams;
@compute @workgroup_size(256)
fn cs_compute_distances(@builtin(global_invocation_id) id: vec3<u32>) {
  let idx = id.x;
  if ((idx >= sort_params.splat_count)) {
    return;
  }
  let pos = splats[idx].position;
  let diff = (pos - sort_params.camera_pos);
  let dist = dot(diff, diff);
  sort_entries[idx] = SortEntry(dist, idx);
}
struct CameraUniform {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    position: vec3<f32>,
    _pad: f32,
}
struct RenderParams {
    viewport_size: vec2<f32>,
    focal_x: f32,
    focal_y: f32,
}
@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<storage, read> render_splats: array<GaussianSplat>;
@group(1) @binding(1) var<storage, read> sorted_indices: array<u32>;
@group(1) @binding(2) var<uniform> render_params: RenderParams;
struct SplatVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) alpha: f32,
    @location(2) quad_pos: vec2<f32>,
    @location(3) conic: vec3<f32>,
}
fn quat_to_mat3(q: vec4<f32>) -> mat3x3<f32> {
  let w = q.x;
  let x = q.y;
  let y = q.z;
  let z = q.w;
  return mat3x3<f32>(
          vec3<f32>(1.0 - 2.0*(y*y + z*z), 2.0*(x*y + w*z), 2.0*(x*z - w*y)),
          vec3<f32>(2.0*(x*y - w*z), 1.0 - 2.0*(x*x + z*z), 2.0*(y*z + w*x)),
          vec3<f32>(2.0*(x*z + w*y), 2.0*(y*z - w*x), 1.0 - 2.0*(x*x + y*y)),
      );
}
@vertex
fn vs_splat(
    @builtin(vertex_index) vertex_id: u32,
    @builtin(instance_index) instance_id: u32,
) -> SplatVertexOutput {
    let splat_idx = sorted_indices[instance_id];
    let splat = render_splats[splat_idx];
    let rot = quat_to_mat3(splat.rotation);
    let scale = exp(splat.scale);
    let s = mat3x3<f32>(
        vec3<f32>(scale.x, 0.0, 0.0),
        vec3<f32>(0.0, scale.y, 0.0),
        vec3<f32>(0.0, 0.0, scale.z),
    );
    let m = (rot * s);
    let sigma = (m * transpose(m));
    let view_pos = (camera.view * vec4<f32>(splat.position, 1.0));
    let tz = view_pos.z;
    let fx = render_params.focal_x;
    let fy = render_params.focal_y;
    let j = mat3x3<f32>(
        vec3<f32>(fx / tz, 0.0, 0.0),
        vec3<f32>(0.0, fy / tz, 0.0),
        vec3<f32>(-fx * view_pos.x / (tz * tz), -fy * view_pos.y / (tz * tz), 0.0),
    );
    let view_rot = mat3x3<f32>(
        camera.view[0].xyz,
        camera.view[1].xyz,
        camera.view[2].xyz,
    );
    let t = (j * view_rot);
    let cov2d = (t * sigma * transpose(t));
    let a = (cov2d[0][0] + 0.3);
    let b = cov2d[0][1];
    let c = (cov2d[1][1] + 0.3);
    let det = ((a * c) - (b * b));
    let det_inv = (1.0 / max(det, 1e-6));
    let trace = (a + c);
    let disc = max(((trace * trace * 0.25) - det), 0.0);
    let lambda1 = ((trace * 0.5) + sqrt(disc));
    let lambda2 = ((trace * 0.5) - sqrt(disc));
    let radius = ceil((3.0 * sqrt(max(lambda1, lambda2))));
    let offsets = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = offsets[vertex_id];
    let ndc = (camera.projection * view_pos);
    let pixel_center = (((ndc.xy / ndc.w) * 0.5) + 0.5);
    let screen_pos = (pixel_center * render_params.viewport_size);
    let offset_screen = (screen_pos + (corner * radius));
    let final_ndc = (((offset_screen / render_params.viewport_size) * 2.0) - 1.0);
    var out: SplatVertexOutput;
    out.position = vec4<f32>(final_ndc.x, (-final_ndc.y), (ndc.z / ndc.w), 1.0);
    out.color = max((splat.sh_dc + 0.5), vec3<f32>(0.0));
    out.alpha = (1.0 / (1.0 + exp((-splat.opacity))));
    out.quad_pos = (corner * radius);
    out.conic = vec3<f32>((c * det_inv), ((-b) * det_inv), (a * det_inv));
    return out;
}
@fragment
fn fs_splat(in: SplatVertexOutput) -> @location(0) vec4<f32> {
  let d = in.quad_pos;
  let power = ((-0.5) * ((in.conic.x * d.x * d.x) + (2.0 * in.conic.y * d.x * d.y) + (in.conic.z * d.y * d.y)));
  if ((power > 0.0)) {
    discard;
  }
  let gaussian = exp(power);
  let alpha = min(0.99, (in.alpha * gaussian));
  if ((alpha < (1.0 / 255.0))) {
    discard;
  }
  return vec4<f32>((in.color * alpha), alpha);
}
