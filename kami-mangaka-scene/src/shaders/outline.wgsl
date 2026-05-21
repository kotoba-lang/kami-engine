// P2 outline pass — Sobel-style luminance gradient on the base render.
// Output is white where edges are detected, black elsewhere. Encodes to
// `outline_blob_key` in vertex_mangaka_scene_3d for manga-style inking.

@group(0) @binding(0) var src_tex     : texture_2d<f32>;
@group(0) @binding(1) var src_sampler : sampler;

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0)       uv       : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi : u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var o : VsOut;
    let p = pos[vi];
    o.clip_pos = vec4<f32>(p, 0.0, 1.0);
    o.uv = vec2<f32>((p.x + 1.0) * 0.5, 1.0 - (p.y + 1.0) * 0.5);
    return o;
}

fn lum(c : vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fs_main(inp : VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(src_tex, 0));
    let px = 1.0 / dims;
    let uv = inp.uv;

    let tl = lum(textureSample(src_tex, src_sampler, uv + vec2<f32>(-px.x, -px.y)).rgb);
    let t  = lum(textureSample(src_tex, src_sampler, uv + vec2<f32>(  0.0, -px.y)).rgb);
    let tr = lum(textureSample(src_tex, src_sampler, uv + vec2<f32>( px.x, -px.y)).rgb);
    let l  = lum(textureSample(src_tex, src_sampler, uv + vec2<f32>(-px.x,   0.0)).rgb);
    let r  = lum(textureSample(src_tex, src_sampler, uv + vec2<f32>( px.x,   0.0)).rgb);
    let bl = lum(textureSample(src_tex, src_sampler, uv + vec2<f32>(-px.x,  px.y)).rgb);
    let b  = lum(textureSample(src_tex, src_sampler, uv + vec2<f32>(  0.0,  px.y)).rgb);
    let br = lum(textureSample(src_tex, src_sampler, uv + vec2<f32>( px.x,  px.y)).rgb);

    let gx = (tr + 2.0 * r + br) - (tl + 2.0 * l + bl);
    let gy = (bl + 2.0 * b + br) - (tl + 2.0 * t + tr);
    let g = sqrt(gx * gx + gy * gy);

    // Threshold to a binary mask, invert so lines are black on white (manga ink).
    let edge = step(0.25, g);
    let ink = 1.0 - edge;
    return vec4<f32>(ink, ink, ink, 1.0);
}
