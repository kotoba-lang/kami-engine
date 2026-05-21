// P2 headless render — sky gradient + character silhouettes via per-pixel ray
// intersection. Each character is approximated as a vertical capsule centered
// at the root_xform translation, sized for a humanoid (height 1.7 m, radius
// 0.25 m). Outline pass lands in P2.1 (Sobel on depth).

const MAX_CHARS : u32 = 16u;

struct Uniforms {
    view_proj_inv : mat4x4<f32>,
    cam_origin    : vec4<f32>,   // xyz = camera world position
    sun_dir       : vec4<f32>,   // xyz = sun direction (toward sun)
    sky_top       : vec4<f32>,
    sky_bottom    : vec4<f32>,
    ground        : vec4<f32>,   // xyz colour, w = horizon y (in world space)
    char_count    : vec4<u32>,   // x = count
    chars         : array<vec4<f32>, MAX_CHARS>,        // xyz = capsule center, w = capsule half-height
    chars_radius  : array<vec4<f32>, MAX_CHARS>,        // x = radius, yzw = unused
    chars_colour  : array<vec4<f32>, MAX_CHARS>,        // rgba (silhouette tint)
};

@group(0) @binding(0) var<uniform> u : Uniforms;

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0)       uv       : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi : u32) -> VsOut {
    // Fullscreen triangle.
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var o : VsOut;
    let p = pos[vi];
    o.clip_pos = vec4<f32>(p, 0.0, 1.0);
    // Flip y so (0,0) is top-left in image space, matching PNG raster order.
    o.uv = vec2<f32>((p.x + 1.0) * 0.5, 1.0 - (p.y + 1.0) * 0.5);
    return o;
}

fn ray_dir(uv : vec2<f32>) -> vec3<f32> {
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    // Unproject two clip-space points (z = 0 near, z = 1 far) and take the diff.
    let near = u.view_proj_inv * vec4<f32>(ndc, 0.0, 1.0);
    let far  = u.view_proj_inv * vec4<f32>(ndc, 1.0, 1.0);
    let n = near.xyz / near.w;
    let f = far.xyz  / far.w;
    return normalize(f - n);
}

// Ray vs vertical capsule (axis = +Y), centered at `c`. half_h = half-height of
// the cylindrical core (sphere caps are added). Returns nearest positive t, or
// -1.0 if miss.
fn intersect_capsule(ro: vec3<f32>, rd: vec3<f32>, c: vec3<f32>, half_h: f32, r: f32) -> f32 {
    // Closed-form: project ray onto the YZ-symmetry. Decompose into
    // perpendicular plane components.
    let oc = ro - c;
    // Cylinder body: x^2 + z^2 = r^2, with |y - cy| <= half_h.
    let a = rd.x * rd.x + rd.z * rd.z;
    let b = 2.0 * (oc.x * rd.x + oc.z * rd.z);
    let cc = oc.x * oc.x + oc.z * oc.z - r * r;
    var t_hit = -1.0;

    if (a > 1e-6) {
        let disc = b * b - 4.0 * a * cc;
        if (disc >= 0.0) {
            let sq = sqrt(disc);
            let t0 = (-b - sq) / (2.0 * a);
            let t1 = (-b + sq) / (2.0 * a);
            for (var i = 0; i < 2; i = i + 1) {
                let tx = select(t1, t0, i == 0);
                if (tx > 1e-3) {
                    let y = ro.y + tx * rd.y;
                    if (abs(y - c.y) <= half_h) {
                        if (t_hit < 0.0 || tx < t_hit) { t_hit = tx; }
                        break;
                    }
                }
            }
        }
    }
    // Spherical caps at c ± half_h * Y.
    for (var s = 0; s < 2; s = s + 1) {
        let cy = c.y + select(-half_h, half_h, s == 1);
        let cs = vec3<f32>(c.x, cy, c.z);
        let oc2 = ro - cs;
        let b2 = 2.0 * dot(rd, oc2);
        let c2 = dot(oc2, oc2) - r * r;
        let disc2 = b2 * b2 - 4.0 * c2;
        if (disc2 >= 0.0) {
            let sq2 = sqrt(disc2);
            let t0 = (-b2 - sq2) * 0.5;
            if (t0 > 1e-3 && (t_hit < 0.0 || t0 < t_hit)) {
                t_hit = t0;
            }
        }
    }
    return t_hit;
}

@fragment
fn fs_main(inp : VsOut) -> @location(0) vec4<f32> {
    let ro = u.cam_origin.xyz;
    let rd = ray_dir(inp.uv);

    let n = u.char_count.x;
    var t_min : f32 = 1.0e30;
    var idx : i32 = -1;

    for (var i = 0u; i < n; i = i + 1u) {
        let ci = u.chars[i];
        let r  = u.chars_radius[i].x;
        let t  = intersect_capsule(ro, rd, ci.xyz, ci.w, r);
        if (t > 0.0 && t < t_min) {
            t_min = t;
            idx = i32(i);
        }
    }

    // Ground plane at y = ground.w.
    let ground_y = u.ground.w;
    var t_ground : f32 = -1.0;
    if (abs(rd.y) > 1e-6) {
        let t = (ground_y - ro.y) / rd.y;
        if (t > 1e-3) { t_ground = t; }
    }

    if (idx >= 0 && (t_ground < 0.0 || t_min < t_ground)) {
        // Character silhouette with simple wrap-around lambert keyed off sun_dir.
        let ci = u.chars[u32(idx)];
        let hit = ro + rd * t_min;
        let n_approx = normalize(vec3<f32>(hit.x - ci.x, 0.0, hit.z - ci.z));
        let lambert = max(0.4, dot(n_approx, normalize(u.sun_dir.xyz)) * 0.5 + 0.6);
        let col = u.chars_colour[u32(idx)].rgb * lambert;
        return vec4<f32>(col, 1.0);
    }
    if (t_ground > 0.0) {
        // Distance-attenuated ground tone.
        let fog = 1.0 / (1.0 + t_ground * 0.005);
        let mix_amt = clamp(1.0 - fog, 0.0, 0.8);
        let col = mix(u.ground.rgb, u.sky_bottom.rgb, mix_amt);
        return vec4<f32>(col, 1.0);
    }

    // Sky gradient.
    let h = clamp(rd.y * 0.5 + 0.5, 0.0, 1.0);
    let sun_glow = pow(max(0.0, dot(rd, normalize(u.sun_dir.xyz))), 32.0) * 0.6;
    let col = mix(u.sky_bottom.rgb, u.sky_top.rgb, h) + vec3<f32>(sun_glow);
    return vec4<f32>(col, 1.0);
}
