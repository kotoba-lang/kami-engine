struct Uniforms {
    view_proj : mat4x4<f32>,
    cam_pos   : vec4<f32>,
    color     : vec4<f32>,
    light_dir : vec4<f32>,
    // params.x = time (seconds); .y = flake density; .z = flake scale;
    // .w = clear-coat strength (0..1).
    params    : vec4<f32>,
};

@group(0) @binding(0) var<uniform> u : Uniforms;
@group(0) @binding(1) var sky_cube : texture_cube<f32>;
@group(0) @binding(2) var sky_samp : sampler;

struct VsIn  { @location(0) pos : vec3<f32>, @location(1) col : vec3<f32> };
struct VsOut { @builtin(position) clip : vec4<f32>, @location(0) col : vec3<f32> };

@vertex fn vs(in : VsIn) -> VsOut {
    var o : VsOut;
    o.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    o.col  = in.col;
    return o;
}

@fragment fn fs(in : VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.col * u.color.rgb, 1.0);
}

// ── Filled-triangle body panel — PBR + clear-coat metallic-flake ──

struct TriIn  {
    @location(0) pos    : vec3<f32>,
    @location(1) normal : vec3<f32>,
    @location(2) col    : vec4<f32>, // rgb + alpha (alpha < 0.95 → glass / underbody, skips flake)
};
struct TriOut {
    @builtin(position) clip      : vec4<f32>,
    @location(0)       col       : vec4<f32>,
    @location(1)       normal    : vec3<f32>,
    @location(2)       world_pos : vec3<f32>,
};

@vertex fn vs_tri(in : TriIn) -> TriOut {
    var o : TriOut;
    o.clip      = u.view_proj * vec4<f32>(in.pos, 1.0);
    o.col       = in.col;
    o.normal    = in.normal;
    o.world_pos = in.pos;
    return o;
}

// 3D hash → noise for the flake field. World-space sample so the
// flakes are anchored to the body, not the screen — they parallax with
// camera motion exactly like real metallic paint.
fn hash33(p: vec3<f32>) -> vec3<f32> {
    var q = vec3<f32>(
        dot(p, vec3<f32>(127.1, 311.7, 74.7)),
        dot(p, vec3<f32>(269.5, 183.3, 246.1)),
        dot(p, vec3<f32>(113.5, 271.9, 124.6)),
    );
    return fract(sin(q) * 43758.5453);
}

// Simple 3D value noise.
fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = hash33(i + vec3<f32>(0.0, 0.0, 0.0)).x;
    let n100 = hash33(i + vec3<f32>(1.0, 0.0, 0.0)).x;
    let n010 = hash33(i + vec3<f32>(0.0, 1.0, 0.0)).x;
    let n110 = hash33(i + vec3<f32>(1.0, 1.0, 0.0)).x;
    let n001 = hash33(i + vec3<f32>(0.0, 0.0, 1.0)).x;
    let n101 = hash33(i + vec3<f32>(1.0, 0.0, 1.0)).x;
    let n011 = hash33(i + vec3<f32>(0.0, 1.0, 1.0)).x;
    let n111 = hash33(i + vec3<f32>(1.0, 1.0, 1.0)).x;
    let nx00 = mix(n000, n100, u.x);
    let nx10 = mix(n010, n110, u.x);
    let nx01 = mix(n001, n101, u.x);
    let nx11 = mix(n011, n111, u.x);
    let nxy0 = mix(nx00, nx10, u.y);
    let nxy1 = mix(nx01, nx11, u.y);
    return mix(nxy0, nxy1, u.z);
}

// GGX / Trowbridge-Reitz normal distribution.
fn d_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159265 * denom * denom, 1e-6);
}

// Schlick Fresnel.
fn f_schlick(v_dot_h: f32, f0: vec3<f32>) -> vec3<f32> {
    let p = pow(clamp(1.0 - v_dot_h, 0.0, 1.0), 5.0);
    return f0 + (vec3<f32>(1.0) - f0) * p;
}

// Smith / Schlick-GGX geometry term, height-correlated approximation.
fn g_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let gv = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let gl = n_dot_l / (n_dot_l * (1.0 - k) + k);
    return gv * gl;
}

// Real cubemap-backed IBL — the procedural sky math now lives CPU-side
// (`build_sky_cubemap` in lib.rs) and gets baked into a 6×64×64 RGBA8
// cubemap at init. Fragment shader just samples it with the reflection
// direction; bilinear filtering on the GPU handles the smooth gradient
// for free, and a future swap to `kami-atmosphere`'s real sky cubemap
// only changes the CPU bake step — the shader doesn't move.
fn sample_sky(r: vec3<f32>) -> vec3<f32> {
    return textureSample(sky_cube, sky_samp, r).rgb;
}

@fragment fn fs_tri(in : TriOut) -> @location(0) vec4<f32> {
    let n  = normalize(in.normal);
    let v  = normalize(u.cam_pos.xyz - in.world_pos);
    let l  = normalize(u.light_dir.xyz);
    let h  = normalize(v + l);
    let n_dot_l = clamp(dot(n, l), 0.0, 1.0);
    let n_dot_v = clamp(dot(n, v), 1e-4, 1.0);
    let n_dot_h = clamp(dot(n, h), 0.0, 1.0);
    let v_dot_h = clamp(dot(v, h), 0.0, 1.0);

    let base = in.col.rgb;

    // alpha < 0.95 marks glass / underbody — skip flake, blend extra.
    let is_paint = step(0.95, in.col.a);

    // ── flake field ──
    let flake_density = u.params.y;
    let flake_scale   = u.params.z;
    let nf = noise3(in.world_pos * flake_scale);
    // Threshold: only the brightest peaks become flakes; density
    // controls the threshold (0 → no flakes, 1 → full coverage).
    let flake_mask = smoothstep(1.0 - flake_density, 1.0, nf);
    // Per-flake micro-normal jitter — gives each flake a different
    // orientation so the highlight is not uniform.
    let jitter = (hash33(floor(in.world_pos * flake_scale)) - 0.5) * 0.6;
    let n_flake = normalize(n + jitter * flake_mask);

    // ── base layer (metallic-ish paint) ──
    let metallic = 0.50 * is_paint;
    let roughness = mix(0.55, 0.30, is_paint);
    let f0 = mix(vec3<f32>(0.04), base, metallic);

    let n_dot_h_p = clamp(dot(n_flake, h), 0.0, 1.0);
    let d = d_ggx(n_dot_h_p, roughness);
    let f = f_schlick(v_dot_h, f0);
    let g = g_smith(n_dot_v, n_dot_l, roughness);
    let spec = d * f * g / (4.0 * n_dot_v * max(n_dot_l, 1e-4));
    let kd = (vec3<f32>(1.0) - f) * (1.0 - metallic);
    let diffuse = base / 3.14159265;

    // Direct lighting + simple ambient + flake highlight contribution.
    let direct = (kd * diffuse + spec) * n_dot_l * vec3<f32>(1.05, 1.0, 0.95);
    let ambient = base * 0.18;
    let r_vec = reflect(-v, n_flake);
    let ibl = sample_sky(r_vec) * f * (0.4 + 0.6 * (1.0 - roughness)) * is_paint;

    var color = ambient + direct + ibl;

    // ── clear coat (second specular lobe, very low roughness) ──
    let cc = u.params.w * is_paint;
    if (cc > 0.0) {
        let cc_rough = 0.05;
        let cc_d = d_ggx(n_dot_h, cc_rough);
        let cc_f = f_schlick(v_dot_h, vec3<f32>(0.04));
        let cc_g = g_smith(n_dot_v, n_dot_l, cc_rough);
        let cc_spec = cc_d * cc_f * cc_g / (4.0 * n_dot_v * max(n_dot_l, 1e-4));
        let cc_ibl = sample_sky(reflect(-v, n)) * cc_f * 0.35;
        color = color + cc * (cc_spec * n_dot_l + cc_ibl);
    }

    // Reinhard-ish tone-map so highlights don't wash to pure white.
    color = color / (vec3<f32>(1.0) + color);

    return vec4<f32>(color, in.col.a);
}

// ── Ground tile pipeline (textured by surface kind) ──

struct GroundIn {
    @location(0) pos       : vec3<f32>,
    @location(1) col       : vec3<f32>,
    @location(2) surface_id: f32,
};
struct GroundOut {
    @builtin(position) clip       : vec4<f32>,
    @location(0)       col        : vec3<f32>,
    @location(1)       world_xz   : vec2<f32>,
    @location(2)       surface_id : f32,
};

@vertex fn vs_ground(in : GroundIn) -> GroundOut {
    var o : GroundOut;
    o.clip       = u.view_proj * vec4<f32>(in.pos, 1.0);
    o.col        = in.col;
    o.world_xz   = vec2<f32>(in.pos.x, in.pos.z);
    o.surface_id = in.surface_id;
    return o;
}

fn hash21(p: vec2<f32>) -> f32 {
    let h = sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453;
    return fract(h);
}
fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

@fragment fn fs_ground(in : GroundOut) -> @location(0) vec4<f32> {
    let p = in.world_xz;
    let id = i32(in.surface_id + 0.5);
    var col = in.col;
    var alpha = 1.0;
    if (id == 0) {
        let grain = noise2(p * 6.0) * 0.10 - 0.05;
        col = col + vec3<f32>(grain);
        let line = step(abs(p.x), 0.15) * step(0.5, fract(p.y * 0.20));
        col = mix(col, vec3<f32>(0.85, 0.78, 0.20), line * 0.8);
    } else if (id == 1) {
        let g = noise2(p * 4.0) * 0.18;
        col = col + vec3<f32>(g, g * 1.1, g * 1.3);
    } else if (id == 2) {
        let s = step(0.6, noise2(p * 12.0)) * 0.25;
        col = col + vec3<f32>(s);
    } else if (id == 3) {
        let r = sin(p.x * 6.0) * 0.05 + sin(p.y * 8.0) * 0.04;
        col = col + vec3<f32>(r, r * 0.9, r * 0.5);
        let grain = (noise2(p * 30.0) - 0.5) * 0.06;
        col = col + vec3<f32>(grain);
    } else if (id == 4) {
        let d = noise2(p * 2.5) * 0.10;
        col = col - vec3<f32>(d * 0.5);
    } else if (id == 5) {
        let cracks = step(0.92, noise2(p * 8.0));
        col = col - vec3<f32>(cracks * 0.5);
        col = col + vec3<f32>(0.0, 0.02, 0.04 + sin(p.y * 0.3) * 0.04);
    } else if (id == 6) {
        let mud = noise2(p * 4.0) * 0.30;
        col = col - vec3<f32>(mud * 0.5, mud * 0.4, mud * 0.3);
    } else {
        let tuft = step(0.75, noise2(p * 10.0)) * 0.20;
        let dark = noise2(p * 3.0) * 0.20 - 0.10;
        col = col + vec3<f32>(-dark * 0.4, tuft - dark, -dark * 0.4);
    }
    return vec4<f32>(col, alpha);
}
