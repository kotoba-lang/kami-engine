//! hikari-rt (光) — WebGPU ray tracing primitive: the native arm of `kami.rt`.
//!
//! `kami.rt` (clj/edn) is the brain — it authors an EDN ray-tracing recipe and
//! normalizes it to a backend-neutral IR. This crate is the **WebGPU executor**:
//! it builds the acceleration structure ([`bvh`]) and generates the WGSL
//! ray-query trace shader ([`wgsl_ray_query`]) from the same integrator
//! parameters. GPU dispatch (pipeline/bind-group creation on a `wgpu::Device`)
//! is the host's integration step; everything here is GPU-free and unit-tested.
//!
//! ADR-2605261800: R1.0 path reservation → R1.2 brings PSNR ≥ 35 dB vs Mitsuba 3.

pub mod bvh;
pub mod gpu;

pub const ADR: &str = "ADR-2605261800";
pub const PHASE: &str = "R1.2-cpu-accel+wgsl-gen";
pub const KAMI_NAME: &str = "hikari-rt";
pub const NV_COMPAT_TARGET: &str = "OptiX";

/// Path-tracer integrator parameters (mirrors clj `:rt/integrator` + `:rt/sampler`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RtConfig {
    pub max_bounces: u32,
    pub spp: u32,
    pub clamp: f32,
    pub seed: u32,
}

impl Default for RtConfig {
    fn default() -> Self {
        Self {
            max_bounces: 4,
            spp: 8,
            clamp: 10.0,
            seed: 0,
        }
    }
}

/// Format a number as a WGSL f32 literal (integers gain a trailing `.0`).
fn fnum(x: f32) -> String {
    if x.fract() == 0.0 {
        format!("{}.0", x as i64)
    } else {
        format!("{x}")
    }
}

/// Generate the WGSL ray-query compute shader for `cfg`. Integrator parameters
/// are baked as `override` constants so one IR recompiles per quality preset.
/// This is the host-side counterpart of clj `kami.rt/emit-wgsl` — the same
/// shader either side can emit, kept here so the native host needs no clj.
pub fn wgsl_ray_query(name: &str, cfg: &RtConfig) -> String {
    format!(
        "// kami-rt — generated WGSL ray-query trace for recipe \"{name}\"\n\
         enable chromium_experimental_ray_query;\n\n\
         struct RtGlobals {{\n  \
           inv_view_proj: mat4x4<f32>,\n  \
           cam_pos: vec3<f32>,\n  \
           frame: u32,\n  \
           width: u32,\n  \
           height: u32,\n\
         }};\n\n\
         override RT_MAX_BOUNCES: u32 = {bounces}u;\n\
         override RT_SPP: u32 = {spp}u;\n\
         override RT_CLAMP: f32 = {clamp};\n\
         override RT_SEED: u32 = {seed}u;\n\n\
         @group(0) @binding(0) var tlas: acceleration_structure;\n\
         @group(0) @binding(1) var<uniform> u: RtGlobals;\n\
         @group(0) @binding(2) var<storage, read_write> out_color: array<vec4<f32>>;\n\n\
         @compute @workgroup_size(8, 8, 1)\n\
         fn trace(@builtin(global_invocation_id) gid: vec3<u32>) {{\n  \
           if (gid.x >= u.width || gid.y >= u.height) {{ return; }}\n  \
           let idx = gid.y * u.width + gid.x;\n  \
           var radiance = vec3<f32>(0.0);\n  \
           for (var s: u32 = 0u; s < RT_SPP; s = s + 1u) {{\n    \
             var ray = primary_ray(gid.xy, s);\n    \
             var throughput = vec3<f32>(1.0);\n    \
             for (var b: u32 = 0u; b <= RT_MAX_BOUNCES; b = b + 1u) {{\n      \
               var rq: ray_query;\n      \
               rayQueryInitialize(&rq, tlas, ray);\n      \
               rayQueryProceed(&rq);\n      \
               let hit = rayQueryGetCommittedIntersection(&rq);\n      \
               if (hit.kind == RAY_QUERY_INTERSECTION_NONE) {{\n        \
                 radiance = radiance + throughput * sky(ray.dir);\n        \
                 break;\n      \
               }}\n      \
               radiance = radiance + throughput * emission(hit);\n      \
               throughput = throughput * bsdf_sample(hit, &ray, s, b);\n    \
             }}\n  \
           }}\n  \
           radiance = min(radiance / f32(RT_SPP), vec3<f32>(RT_CLAMP));\n  \
           out_color[idx] = vec4<f32>(radiance, 1.0);\n\
         }}\n",
        bounces = cfg.max_bounces,
        spp = cfg.spp,
        clamp = fnum(cfg.clamp),
        seed = cfg.seed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wgsl_bakes_integrator_params() {
        let cfg = RtConfig {
            max_bounces: 6,
            spp: 16,
            clamp: 8.0,
            seed: 7,
        };
        let src = wgsl_ray_query("gi", &cfg);
        assert!(src.contains("RT_MAX_BOUNCES: u32 = 6u"));
        assert!(src.contains("RT_SPP: u32 = 16u"));
        assert!(src.contains("RT_CLAMP: f32 = 8.0"));
        assert!(src.contains("RT_SEED: u32 = 7u"));
        assert!(src.contains("ray_query"));
        assert!(src.contains("rayQueryInitialize"));
        assert!(src.contains("struct RtGlobals"));
    }

    #[test]
    fn default_config_matches_clj_defaults() {
        let c = RtConfig::default();
        assert_eq!((c.max_bounces, c.spp, c.seed), (4, 8, 0));
        assert_eq!(c.clamp, 10.0);
    }
}
