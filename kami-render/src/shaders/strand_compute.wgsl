// Strand Hair Compute Shader — 100K strands → screen-space quad expansion.
//
// Pipeline:
//   1. Compute pass: read strand SSBO → expand each segment to 2 triangles (screen-space quads)
//   2. Render pass: draw expanded quads with Kajiya-Kay BRDF
//
// SSBO layout:
//   points: [x, y, z, width] × N_total_points (flat array)
//   offsets: [u32] × (N_strands + 1) — strand_i points at [offsets[i]..offsets[i+1])
//
// Output: vertex buffer with position + tangent + uv + color per expanded quad vertex

// ─── Compute Pass: Strand → Quad Expansion ───

struct StrandParams {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    camera_pos: vec3<f32>,
    screen_width: f32,
    screen_height: f32,
    strand_count: u32,
    total_points: u32,
    time: f32,
    // Hair properties
    base_color: vec3<f32>,
    root_darken: f32,
    highlight_color: vec3<f32>,
    highlight_ratio: f32,
    // Wind
    wind_x: f32,
    wind_z: f32,
    wind_strength: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> params: StrandParams;
@group(0) @binding(1) var<storage, read> points: array<vec4<f32>>;      // xyz + width
@group(0) @binding(2) var<storage, read> offsets: array<u32>;           // per-strand start
@group(0) @binding(3) var<storage, read_write> quads: array<f32>;       // output: 6 verts × 12 floats per segment

// Each workgroup processes one strand segment → 2 triangles (6 vertices)
// Vertex layout: pos(3) + tangent(3) + uv(2) + color(4) = 12 floats
const FLOATS_PER_VERT: u32 = 12u;
const VERTS_PER_SEGMENT: u32 = 6u;
const FLOATS_PER_SEGMENT: u32 = 72u; // 6 × 12

@compute @workgroup_size(64)
fn cs_expand(@builtin(global_invocation_id) gid: vec3<u32>) {
    let strand_idx = gid.x;
    if (strand_idx >= params.strand_count) { return; }

    let start = offsets[strand_idx];
    let end = offsets[strand_idx + 1u];
    let n_points = end - start;
    if (n_points < 2u) { return; }

    // Determine if this is a highlight strand (based on strand index hash)
    let is_highlight = f32(strand_idx % 100u) / 100.0 > (1.0 - params.highlight_ratio);
    let base_col = select(params.base_color, params.highlight_color, is_highlight);

    // Process each segment (pair of consecutive points)
    for (var seg = 0u; seg < n_points - 1u; seg++) {
        let pi0 = start + seg;
        let pi1 = start + seg + 1u;
        let p0_raw = points[pi0];
        let p1_raw = points[pi1];

        // World-space positions
        var p0 = (params.model * vec4<f32>(p0_raw.xyz, 1.0)).xyz;
        var p1 = (params.model * vec4<f32>(p1_raw.xyz, 1.0)).xyz;

        // Wind displacement (increases along strand)
        let t0 = f32(seg) / f32(n_points - 1u);
        let t1 = f32(seg + 1u) / f32(n_points - 1u);
        let wind_factor0 = t0 * t0 * params.wind_strength;
        let wind_factor1 = t1 * t1 * params.wind_strength;
        p0.x += params.wind_x * wind_factor0;
        p0.z += params.wind_z * wind_factor0;
        p1.x += params.wind_x * wind_factor1;
        p1.z += params.wind_z * wind_factor1;

        let width0 = p0_raw.w;
        let width1 = p1_raw.w;

        // Tangent along strand
        let tangent = normalize(p1 - p0);

        // Billboard: expand perpendicular to camera view direction
        let view_dir = normalize(params.camera_pos - (p0 + p1) * 0.5);
        let right = normalize(cross(tangent, view_dir));

        // Quad corners
        let l0 = p0 - right * width0;
        let r0 = p0 + right * width0;
        let l1 = p1 - right * width1;
        let r1 = p1 + right * width1;

        // Root-to-tip color (darker at root)
        let col0 = base_col * mix(params.root_darken, 1.0, sqrt(t0));
        let col1 = base_col * mix(params.root_darken, 1.0, sqrt(t1));

        // Alpha (fade at tips)
        let alpha0 = 1.0;
        let alpha1 = select(1.0, max(0.0, 1.0 - (t1 - 0.7) / 0.3), t1 > 0.7);

        // Output: 6 vertices (2 triangles: l0-l1-r0, r0-l1-r1)
        let out_base = (strand_idx * (n_points - 1u) + seg) * FLOATS_PER_SEGMENT;

        // Triangle 1: l0, l1, r0
        write_vert(out_base + 0u * FLOATS_PER_VERT, l0, tangent, vec2<f32>(0.0, t0), vec4<f32>(col0, alpha0));
        write_vert(out_base + 1u * FLOATS_PER_VERT, l1, tangent, vec2<f32>(0.0, t1), vec4<f32>(col1, alpha1));
        write_vert(out_base + 2u * FLOATS_PER_VERT, r0, tangent, vec2<f32>(1.0, t0), vec4<f32>(col0, alpha0));
        // Triangle 2: r0, l1, r1
        write_vert(out_base + 3u * FLOATS_PER_VERT, r0, tangent, vec2<f32>(1.0, t0), vec4<f32>(col0, alpha0));
        write_vert(out_base + 4u * FLOATS_PER_VERT, l1, tangent, vec2<f32>(0.0, t1), vec4<f32>(col1, alpha1));
        write_vert(out_base + 5u * FLOATS_PER_VERT, r1, tangent, vec2<f32>(1.0, t1), vec4<f32>(col1, alpha1));
    }
}

fn write_vert(offset: u32, pos: vec3<f32>, tangent: vec3<f32>, uv: vec2<f32>, color: vec4<f32>) {
    quads[offset + 0u] = pos.x;
    quads[offset + 1u] = pos.y;
    quads[offset + 2u] = pos.z;
    quads[offset + 3u] = tangent.x;
    quads[offset + 4u] = tangent.y;
    quads[offset + 5u] = tangent.z;
    quads[offset + 6u] = uv.x;
    quads[offset + 7u] = uv.y;
    quads[offset + 8u] = color.x;
    quads[offset + 9u] = color.y;
    quads[offset + 10u] = color.z;
    quads[offset + 11u] = color.w; // alpha
}

// ─── Render Pass: Draw Expanded Quads ───

struct RenderParams {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
    light_dir: vec3<f32>,
    light_intensity: f32,
};
@group(0) @binding(0) var<uniform> rparams: RenderParams;

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) tangent: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) world_pos: vec3<f32>,
};

@vertex
fn vs_strand(
    @location(0) pos: vec3<f32>,
    @location(1) tangent: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
) -> VSOut {
    var out: VSOut;
    out.clip_pos = rparams.view_proj * vec4<f32>(pos, 1.0);
    out.tangent = tangent;
    out.uv = uv;
    out.color = color;
    out.world_pos = pos;
    return out;
}

@fragment
fn fs_strand(v: VSOut) -> @location(0) vec4<f32> {
    if (v.color.a < 0.01) { discard; }

    let T = normalize(v.tangent);
    let V = normalize(rparams.camera_pos - v.world_pos);
    let L = normalize(-rparams.light_dir);
    let H = normalize(V + L);

    // Kajiya-Kay diffuse
    let TdotL = dot(T, L);
    let diffuse = sqrt(max(0.0, 1.0 - TdotL * TdotL));

    // Dual specular (R + TRT)
    let TdotH = dot(T, H);
    let sinTH = sqrt(max(0.0, 1.0 - TdotH * TdotH));
    let spec_R = pow(sinTH, 40.0) * 0.4;    // sharp cuticle reflection
    let spec_TRT = pow(sinTH, 15.0) * 0.25; // broad colored highlight

    // Transmission (back-lit glow)
    let transmission = max(0.0, dot(-V, L)) * 0.1;

    // Combine
    let ambient = v.color.rgb * 0.08;
    let color = ambient +
        (v.color.rgb * diffuse +
         vec3<f32>(1.0, 0.98, 0.95) * spec_R +
         v.color.rgb * 1.3 * spec_TRT +
         v.color.rgb * transmission) * rparams.light_intensity;

    // ACES tonemapping
    let aces = (color * (2.51 * color + 0.03)) / (color * (2.43 * color + 0.59) + 0.14);

    return vec4<f32>(pow(aces, vec3<f32>(1.0 / 2.2)), v.color.a);
}
