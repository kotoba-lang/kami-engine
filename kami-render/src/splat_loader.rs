//! PLY and .splat file parsers for 3D Gaussian Splatting.
//!
//! Inline PLY parser (~no external dependency). .splat is antimatter15's 32B compact format.

use crate::splat::{GaussianSplat, SplatCloud};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SplatLoadError {
    #[error("invalid PLY header")]
    InvalidHeader,
    #[error("missing property: {0}")]
    MissingProperty(&'static str),
    #[error("unexpected end of data")]
    UnexpectedEof,
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("bad SPZ magic (not a gzip'd SPZ file)")]
    BadMagic,
    #[error("gzip decompress failed: {0}")]
    Decompress(String),
}

/// Load .splat binary format (antimatter15 compact: 32 bytes per splat).
///
/// Layout per splat (little-endian):
///   position: 3 × f32 (12B)
///   scale:    3 × f32 (12B) — already exp'd
///   color:    4 × u8  (4B)  — RGBA [0,255]
///   rotation: 4 × u8  (4B)  — quaternion normalized to [0,255]
pub fn load_splat(data: &[u8]) -> Result<SplatCloud, SplatLoadError> {
    const STRIDE: usize = 32;
    if data.len() < STRIDE {
        return Ok(SplatCloud::new());
    }

    let count = data.len() / STRIDE;
    let mut cloud = SplatCloud::new();
    cloud.splats.reserve(count);

    for i in 0..count {
        let off = i * STRIDE;
        if off + STRIDE > data.len() {
            break;
        }

        let px = f32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let py = f32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap());
        let pz = f32::from_le_bytes(data[off + 8..off + 12].try_into().unwrap());

        let sx = f32::from_le_bytes(data[off + 12..off + 16].try_into().unwrap());
        let sy = f32::from_le_bytes(data[off + 16..off + 20].try_into().unwrap());
        let sz = f32::from_le_bytes(data[off + 20..off + 24].try_into().unwrap());

        let r = data[off + 24] as f32 / 255.0;
        let g = data[off + 25] as f32 / 255.0;
        let b = data[off + 26] as f32 / 255.0;
        let a = data[off + 27] as f32 / 255.0;

        let qw = (data[off + 28] as f32 / 128.0) - 1.0;
        let qx = (data[off + 29] as f32 / 128.0) - 1.0;
        let qy = (data[off + 30] as f32 / 128.0) - 1.0;
        let qz = (data[off + 31] as f32 / 128.0) - 1.0;

        // Convert color [0,1] → SH DC band (subtract 0.5 for SH convention)
        let sh_dc = [r - 0.5, g - 0.5, b - 0.5];

        // Convert scale to log-space (as expected by renderer)
        let log_scale = [sx.max(1e-8).ln(), sy.max(1e-8).ln(), sz.max(1e-8).ln()];

        // Convert opacity to logit (inverse sigmoid)
        let clamped_a = a.clamp(0.001, 0.999);
        let logit_opacity = (clamped_a / (1.0 - clamped_a)).ln();

        cloud.splats.push(GaussianSplat {
            position: [px, py, pz],
            opacity: logit_opacity,
            scale: log_scale,
            _pad0: 0.0,
            rotation: [qw, qx, qy, qz],
            sh_dc,
            _pad1: 0.0,
        });
    }

    Ok(cloud)
}

/// Load PLY file containing 3D Gaussian Splat data.
///
/// Expected properties: x, y, z, opacity, scale_0/1/2, rot_0/1/2/3, f_dc_0/1/2.
///
/// Supports both ASCII and `binary_little_endian` PLY. The header is
/// always ASCII so we locate `end_header\n` by scanning the raw byte
/// stream and only run UTF-8 validation over the header slice — the
/// binary body after the separator is, by definition, not UTF-8 and
/// must not be passed through `from_utf8`.
pub fn load_ply(data: &[u8]) -> Result<SplatCloud, SplatLoadError> {
    const SEP: &[u8] = b"end_header\n";
    let header_end = data
        .windows(SEP.len())
        .position(|w| w == SEP)
        .ok_or(SplatLoadError::InvalidHeader)?;
    let header =
        std::str::from_utf8(&data[..header_end]).map_err(|_| SplatLoadError::InvalidHeader)?;
    let body_start = header_end + SEP.len();

    // Parse header
    let mut vertex_count = 0u32;
    let mut properties: Vec<(&str, &str)> = Vec::new(); // (type, name)
    let mut format_binary = false;

    for line in header.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == "element" && parts[1] == "vertex" {
            vertex_count = parts[2]
                .parse()
                .map_err(|_| SplatLoadError::InvalidHeader)?;
        } else if parts.len() >= 3 && parts[0] == "property" {
            properties.push((parts[1], parts[2]));
        } else if parts.len() >= 3 && parts[0] == "format" {
            format_binary = parts[1].starts_with("binary");
        }
    }

    if vertex_count == 0 {
        return Ok(SplatCloud::new());
    }

    // Build property index
    let find_prop =
        |name: &str| -> Option<usize> { properties.iter().position(|(_, n)| *n == name) };

    let ix = find_prop("x").ok_or(SplatLoadError::MissingProperty("x"))?;
    let iy = find_prop("y").ok_or(SplatLoadError::MissingProperty("y"))?;
    let iz = find_prop("z").ok_or(SplatLoadError::MissingProperty("z"))?;
    let iopacity = find_prop("opacity");
    let iscale0 = find_prop("scale_0");
    let iscale1 = find_prop("scale_1");
    let iscale2 = find_prop("scale_2");
    let irot0 = find_prop("rot_0");
    let irot1 = find_prop("rot_1");
    let irot2 = find_prop("rot_2");
    let irot3 = find_prop("rot_3");
    let idc0 = find_prop("f_dc_0");
    let idc1 = find_prop("f_dc_1");
    let idc2 = find_prop("f_dc_2");

    // Higher-SH bands. The 3DGS PLY convention emits f_rest_* in
    // channel-major order: for K=(degree+1)² coefficients, the per-
    // splat layout is [R₁..R_{K-1}, G₁..G_{K-1}, B₁..B_{K-1}] — i.e.
    // 3·(K-1) floats. We rearrange to coefficient-major
    // [R₁G₁B₁, R₂G₂B₂, …] for renderer-friendly indexing.
    let mut rest_indices: Vec<usize> = Vec::new();
    {
        let mut i = 0;
        loop {
            let name = format!("f_rest_{i}");
            match find_prop(&name) {
                Some(idx) => {
                    rest_indices.push(idx);
                    i += 1;
                }
                None => break,
            }
        }
    }
    // K-1 = rest_indices.len() / 3. degree = sqrt(K) - 1.
    // Only accept counts matching valid SH degrees ∈ {1, 2, 3}
    // (3, 8, 15 rest coefficients per channel respectively × 3 = 9, 24, 45).
    let (sh_degree_loaded, rest_per_splat) = match rest_indices.len() {
        0 => (0u8, 0usize),
        9 => (1u8, 9usize),   // K=4,  K-1=3,  ×3 channels
        24 => (2u8, 24usize), // K=9,  K-1=8,  ×3 channels
        45 => (3u8, 45usize), // K=16, K-1=15, ×3 channels
        // Any other count → bail to DC-only rather than try to guess.
        _ => (0u8, 0usize),
    };
    let coefs_per_channel = rest_per_splat / 3;

    let mut cloud = SplatCloud::new();
    cloud.splats.reserve(vertex_count as usize);
    cloud.sh_degree = sh_degree_loaded;
    cloud
        .sh_rest
        .reserve(vertex_count as usize * coefs_per_channel);

    if format_binary {
        // Binary little-endian PLY: each property is f32 (4 bytes)
        let body = &data[body_start..];
        let stride = properties.len() * 4;

        for v in 0..vertex_count as usize {
            let base = v * stride;
            if base + stride > body.len() {
                break;
            }

            let read_f32 = |prop_idx: usize| -> f32 {
                let off = base + prop_idx * 4;
                f32::from_le_bytes(body[off..off + 4].try_into().unwrap())
            };

            let position = [read_f32(ix), read_f32(iy), read_f32(iz)];
            let opacity = iopacity.map(|i| read_f32(i)).unwrap_or(1.0);
            let scale = [
                iscale0.map(|i| read_f32(i)).unwrap_or(0.01),
                iscale1.map(|i| read_f32(i)).unwrap_or(0.01),
                iscale2.map(|i| read_f32(i)).unwrap_or(0.01),
            ];
            let rotation = [
                irot0.map(|i| read_f32(i)).unwrap_or(1.0),
                irot1.map(|i| read_f32(i)).unwrap_or(0.0),
                irot2.map(|i| read_f32(i)).unwrap_or(0.0),
                irot3.map(|i| read_f32(i)).unwrap_or(0.0),
            ];
            let sh_dc = [
                idc0.map(|i| read_f32(i)).unwrap_or(0.0),
                idc1.map(|i| read_f32(i)).unwrap_or(0.0),
                idc2.map(|i| read_f32(i)).unwrap_or(0.0),
            ];

            cloud.splats.push(GaussianSplat {
                position,
                opacity,
                scale,
                _pad0: 0.0,
                rotation,
                sh_dc,
                _pad1: 0.0,
            });
            // Higher-SH coefficients: PLY is channel-major, we want
            // coefficient-major. Read R₁..R_{K-1}, G₁..G_{K-1}, B₁..B_{K-1}
            // and emit per-coef [r,g,b] tuples.
            if coefs_per_channel > 0 {
                for c in 0..coefs_per_channel {
                    let r = read_f32(rest_indices[c]);
                    let g = read_f32(rest_indices[coefs_per_channel + c]);
                    let b = read_f32(rest_indices[2 * coefs_per_channel + c]);
                    cloud.sh_rest.push([r, g, b]);
                }
            }
        }
    } else {
        // ASCII PLY
        let body_text =
            std::str::from_utf8(&data[body_start..]).map_err(|_| SplatLoadError::InvalidHeader)?;
        for line in body_text.lines().take(vertex_count as usize) {
            let vals: Vec<f32> = line
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();

            if vals.len() < properties.len() {
                continue;
            }

            let position = [vals[ix], vals[iy], vals[iz]];
            let opacity = iopacity.map(|i| vals[i]).unwrap_or(1.0);
            let scale = [
                iscale0.map(|i| vals[i]).unwrap_or(0.01),
                iscale1.map(|i| vals[i]).unwrap_or(0.01),
                iscale2.map(|i| vals[i]).unwrap_or(0.01),
            ];
            let rotation = [
                irot0.map(|i| vals[i]).unwrap_or(1.0),
                irot1.map(|i| vals[i]).unwrap_or(0.0),
                irot2.map(|i| vals[i]).unwrap_or(0.0),
                irot3.map(|i| vals[i]).unwrap_or(0.0),
            ];
            let sh_dc = [
                idc0.map(|i| vals[i]).unwrap_or(0.0),
                idc1.map(|i| vals[i]).unwrap_or(0.0),
                idc2.map(|i| vals[i]).unwrap_or(0.0),
            ];

            cloud.splats.push(GaussianSplat {
                position,
                opacity,
                scale,
                _pad0: 0.0,
                rotation,
                sh_dc,
                _pad1: 0.0,
            });
            if coefs_per_channel > 0 {
                for c in 0..coefs_per_channel {
                    let r = vals[rest_indices[c]];
                    let g = vals[rest_indices[coefs_per_channel + c]];
                    let b = vals[rest_indices[2 * coefs_per_channel + c]];
                    cloud.sh_rest.push([r, g, b]);
                }
            }
        }
    }

    Ok(cloud)
}

/// SPZ magic number: ASCII "NGSP" as a little-endian `u32`.
const SPZ_MAGIC: u32 = 0x5053_474e;

/// Per-channel SH coefficient count for an SPZ spherical-harmonics degree.
/// (Niantic `dimForDegree`: 0→0, 1→3, 2→8, 3→15, 4→24.)
fn spz_dim_for_degree(degree: u8) -> usize {
    match degree {
        0 => 0,
        1 => 3,
        2 => 8,
        3 => 15,
        4 => 24,
        _ => 0,
    }
}

/// Decode an IEEE-754 binary16 (half) into `f32` (used by SPZ version 1).
fn half_to_f32(h: u16) -> f32 {
    let sign = (h >> 15) & 1;
    let exp = (h >> 10) & 0x1f;
    let mant = h & 0x3ff;
    let val = match exp {
        0 => {
            // subnormal / zero
            (mant as f32) * 2f32.powi(-24)
        }
        0x1f => {
            // inf / nan
            if mant == 0 { f32::INFINITY } else { f32::NAN }
        }
        _ => (1.0 + (mant as f32) / 1024.0) * 2f32.powi(exp as i32 - 15),
    };
    if sign == 1 { -val } else { val }
}

/// Load a Niantic **SPZ** Gaussian-splat file (`.spz`).
///
/// SPZ is a gzip-compressed binary container. This decoder targets the
/// legacy single-gzip-stream format (versions 1–3), which is what every
/// `.spz` file in the wild today uses. (The v4+ multi-stream ZSTD layout
/// is intentionally out of scope — flate2 only provides gzip.)
///
/// On-disk layout (after gunzip):
///   - 16-byte header: magic(u32 "NGSP") · version(u32) · numPoints(u32) ·
///     shDegree(u8) · fractionalBits(u8) · flags(u8) · reserved(u8)
///   - then, tightly packed for all points in order:
///     positions · alphas · colors · scales · rotations · sh
///
/// Per-point sizes & decode (Niantic `unpackGaussians`):
///   - positions: v1 = 3×f16; else 3×int24·LE, value = fixed / 2^fractionalBits
///   - alphas:    1×u8, opacity(logit) = ln(a/(1-a)) with a = u8/255
///   - colors:    3×u8, sh_dc = (u8/255 - 0.5) / 0.15
///   - scales:    3×u8, log-scale = u8/16 - 10
///   - rotations: v<3 = 3×u8 (first-three); v>=3 = 4×u8 (smallest-three)
///   - sh:        dimForDegree(shDegree)·3 × u8, coef = (u8 - 128)/128
pub fn load_spz(data: &[u8]) -> Result<SplatCloud, SplatLoadError> {
    use std::io::Read;

    // gunzip the whole container.
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut raw = Vec::new();
    decoder
        .read_to_end(&mut raw)
        .map_err(|e| SplatLoadError::Decompress(e.to_string()))?;

    if raw.len() < 16 {
        return Err(SplatLoadError::UnexpectedEof);
    }

    let u32_le =
        |b: &[u8], o: usize| -> u32 { u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) };

    let magic = u32_le(&raw, 0);
    if magic != SPZ_MAGIC {
        return Err(SplatLoadError::BadMagic);
    }
    let version = u32_le(&raw, 4);
    let num_points = u32_le(&raw, 8) as usize;
    let sh_degree_raw = raw[12];
    let fractional_bits = raw[13];
    // raw[14] = flags (bit0 = antialiased), raw[15] = reserved — unused here.

    if num_points == 0 {
        return Ok(SplatCloud::new());
    }

    // Per-point byte strides for each section.
    let pos_stride = if version == 1 { 6 } else { 9 };
    let alpha_stride = 1usize;
    let color_stride = 3usize;
    let scale_stride = 3usize;
    let rot_stride = if version >= 3 { 4 } else { 3 };
    let sh_dim = spz_dim_for_degree(sh_degree_raw); // per-channel coef count
    let sh_stride = sh_dim * 3;

    // Section base offsets within the decompressed payload.
    let mut off = 16usize;
    let pos_base = off;
    off += pos_stride * num_points;
    let alpha_base = off;
    off += alpha_stride * num_points;
    let color_base = off;
    off += color_stride * num_points;
    let scale_base = off;
    off += scale_stride * num_points;
    let rot_base = off;
    off += rot_stride * num_points;
    let sh_base = off;
    off += sh_stride * num_points;
    if raw.len() < off {
        return Err(SplatLoadError::UnexpectedEof);
    }

    // SplatCloud caps higher-order SH at degree 3 (15 coefs/channel). If the
    // file carries degree 4, load the first 15 coefficients (bands 1–3) and
    // drop band 4 — SH coefficients are band-ordered so the prefix is valid.
    let loaded_degree = sh_degree_raw.min(3);
    let loaded_coefs = spz_dim_for_degree(loaded_degree); // 0/3/8/15

    let pos_scale = 1.0f32 / ((1u32 << fractional_bits) as f32);

    let mut cloud = SplatCloud::new();
    cloud.splats.reserve(num_points);
    cloud.sh_degree = loaded_degree;
    cloud.sh_rest.reserve(num_points * loaded_coefs);

    for i in 0..num_points {
        // --- position ---
        let position = if version == 1 {
            let b = pos_base + i * 6;
            [
                half_to_f32(u16::from_le_bytes([raw[b], raw[b + 1]])),
                half_to_f32(u16::from_le_bytes([raw[b + 2], raw[b + 3]])),
                half_to_f32(u16::from_le_bytes([raw[b + 4], raw[b + 5]])),
            ]
        } else {
            let b = pos_base + i * 9;
            let read24 = |o: usize| -> f32 {
                let mut v =
                    (raw[o] as i32) | ((raw[o + 1] as i32) << 8) | ((raw[o + 2] as i32) << 16);
                if v & 0x0080_0000 != 0 {
                    v |= -0x0100_0000i32; // sign-extend bit 23
                }
                (v as f32) * pos_scale
            };
            [read24(b), read24(b + 3), read24(b + 6)]
        };

        // --- alpha → logit opacity ---
        let a = (raw[alpha_base + i] as f32 / 255.0).clamp(0.001, 0.999);
        let opacity = (a / (1.0 - a)).ln();

        // --- color → SH DC ---
        let cb = color_base + i * 3;
        let sh_dc = [
            (raw[cb] as f32 / 255.0 - 0.5) / 0.15,
            (raw[cb + 1] as f32 / 255.0 - 0.5) / 0.15,
            (raw[cb + 2] as f32 / 255.0 - 0.5) / 0.15,
        ];

        // --- scale (log-space) ---
        let sb = scale_base + i * 3;
        let scale = [
            raw[sb] as f32 / 16.0 - 10.0,
            raw[sb + 1] as f32 / 16.0 - 10.0,
            raw[sb + 2] as f32 / 16.0 - 10.0,
        ];

        // --- rotation → wxyz quaternion ---
        let rb = rot_base + i * rot_stride;
        let rotation = if version >= 3 {
            spz_unpack_quat_smallest_three([raw[rb], raw[rb + 1], raw[rb + 2], raw[rb + 3]])
        } else {
            // first-three: xyz stored, w reconstructed.
            let x = raw[rb] as f32 / 127.5 - 1.0;
            let y = raw[rb + 1] as f32 / 127.5 - 1.0;
            let z = raw[rb + 2] as f32 / 127.5 - 1.0;
            let w = (1.0 - (x * x + y * y + z * z)).max(0.0).sqrt();
            [w, x, y, z] // SplatCloud convention is wxyz
        };

        cloud.splats.push(GaussianSplat {
            position,
            opacity,
            scale,
            _pad0: 0.0,
            rotation,
            sh_dc,
            _pad1: 0.0,
        });

        // --- spherical harmonics rest (coefficient-major [r,g,b] triples) ---
        // SPZ packs sh per point as shDim coefficients × 3 channels, RGB
        // interleaved — already coefficient-major, matching sh_rest.
        if loaded_coefs > 0 {
            let shb = sh_base + i * sh_stride;
            for c in 0..loaded_coefs {
                let o = shb + c * 3;
                cloud.sh_rest.push([
                    (raw[o] as f32 - 128.0) / 128.0,
                    (raw[o + 1] as f32 - 128.0) / 128.0,
                    (raw[o + 2] as f32 - 128.0) / 128.0,
                ]);
            }
        }
    }

    Ok(cloud)
}

/// Decode an SPZ v3 "smallest-three" packed quaternion (4 bytes → wxyz).
/// Port of Niantic `unpackQuaternionSmallestThree`: top 2 bits select the
/// omitted (largest) component; the other three are 9-bit magnitude + 1 sign
/// bit each, scaled by `1/√2`, and the largest is recovered from unit norm.
fn spz_unpack_quat_smallest_three(r: [u8; 4]) -> [f32; 4] {
    const SQRT1_2: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const C_MASK: u32 = (1 << 9) - 1;
    let mut comp =
        (r[0] as u32) | ((r[1] as u32) << 8) | ((r[2] as u32) << 16) | ((r[3] as u32) << 24);
    let i_largest = (comp >> 30) as usize;
    // rot indexed xyzw (0=x,1=y,2=z,3=w).
    let mut rot = [0.0f32; 4];
    let mut sum_sq = 0.0f32;
    for i in (0..4).rev() {
        if i != i_largest {
            let mag = comp & C_MASK;
            let negbit = (comp >> 9) & 0x1;
            comp >>= 10;
            let mut v = SQRT1_2 * (mag as f32) / (C_MASK as f32);
            if negbit == 1 {
                v = -v;
            }
            rot[i] = v;
            sum_sq += v * v;
        }
    }
    rot[i_largest] = (1.0 - sum_sq).max(0.0).sqrt();
    // rot is xyzw; SplatCloud wants wxyz.
    [rot[3], rot[0], rot[1], rot[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_splat_empty() {
        let cloud = load_splat(&[]).unwrap();
        assert_eq!(cloud.count(), 0);
    }

    fn gzip(raw: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(raw).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn load_spz_v2_one_point() {
        // Hand-build a version-2, degree-0, 1-point SPZ payload.
        let frac_bits = 12u8; // pos scale = 1/4096
        let mut raw = Vec::new();
        raw.extend_from_slice(&SPZ_MAGIC.to_le_bytes()); // magic
        raw.extend_from_slice(&2u32.to_le_bytes()); // version
        raw.extend_from_slice(&1u32.to_le_bytes()); // numPoints
        raw.push(0); // shDegree
        raw.push(frac_bits); // fractionalBits
        raw.push(0); // flags
        raw.push(0); // reserved
        // position: x=1.0 (4096), y=2.0 (8192), z=0.0 — int24 LE
        let p24 = |v: i32| {
            [
                (v & 0xff) as u8,
                ((v >> 8) & 0xff) as u8,
                ((v >> 16) & 0xff) as u8,
            ]
        };
        raw.extend_from_slice(&p24(4096));
        raw.extend_from_slice(&p24(8192));
        raw.extend_from_slice(&p24(0));
        // alpha
        raw.push(230);
        // colors (mid-gray ≈ 0 DC)
        raw.extend_from_slice(&[128, 128, 128]);
        // scales (160 → log-scale 0)
        raw.extend_from_slice(&[160, 160, 160]);
        // rotation first-three: x≈0.498, y=z=0 → w≈0.867
        raw.extend_from_slice(&[191, 128, 128]);
        // (no SH for degree 0)

        let cloud = load_spz(&gzip(&raw)).unwrap();
        assert_eq!(cloud.count(), 1);
        let s = &cloud.splats[0];
        assert!((s.position[0] - 1.0).abs() < 0.01, "x={}", s.position[0]);
        assert!((s.position[1] - 2.0).abs() < 0.01, "y={}", s.position[1]);
        assert!(s.position[2].abs() < 0.01, "z={}", s.position[2]);
        // log-scale 0
        assert!(
            s.scale.iter().all(|v| v.abs() < 0.01),
            "scale={:?}",
            s.scale
        );
        // opacity logit for a≈0.902 is ≈ +2.2 (positive)
        assert!(s.opacity > 1.0, "opacity={}", s.opacity);
        // quaternion wxyz, w largest, x≈0.498
        assert!((s.rotation[0] - 0.867).abs() < 0.02, "w={}", s.rotation[0]);
        assert!((s.rotation[1] - 0.498).abs() < 0.02, "x={}", s.rotation[1]);
        assert_eq!(cloud.sh_degree, 0);
        assert!(cloud.sh_rest.is_empty());
    }

    #[test]
    fn load_spz_with_sh_degree_1() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&SPZ_MAGIC.to_le_bytes());
        raw.extend_from_slice(&2u32.to_le_bytes());
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.push(1); // shDegree=1 → 3 coefs/channel
        raw.push(12);
        raw.push(0);
        raw.push(0);
        raw.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0]); // pos (9B)
        raw.push(200); // alpha
        raw.extend_from_slice(&[128, 128, 128]); // color
        raw.extend_from_slice(&[160, 160, 160]); // scale
        raw.extend_from_slice(&[128, 128, 128]); // rot (3B)
        // sh: 3 coefs × 3 channels = 9 bytes. byte 128 → 0.0; 192 → 0.5
        raw.extend_from_slice(&[192, 128, 128, 128, 128, 128, 128, 128, 128]);

        let cloud = load_spz(&gzip(&raw)).unwrap();
        assert_eq!(cloud.sh_degree, 1);
        assert_eq!(cloud.sh_rest.len(), 3); // 3 coefs/channel for 1 point
        // first coef R: (192-128)/128 = 0.5
        assert!(
            (cloud.sh_rest[0][0] - 0.5).abs() < 0.01,
            "{:?}",
            cloud.sh_rest[0]
        );
        assert!(cloud.sh_rest[0][1].abs() < 0.01);
    }

    #[test]
    fn load_spz_rejects_non_spz() {
        // Gzip of arbitrary bytes whose first 4 decompressed bytes aren't NGSP.
        let bad = gzip(b"this is not an spz file at all, just text padding............");
        assert!(matches!(load_spz(&bad), Err(SplatLoadError::BadMagic)));
    }

    #[test]
    fn load_splat_one() {
        // 32 bytes: position(12) + scale(12) + color(4) + rotation(4)
        let mut data = vec![0u8; 32];
        // position = (1.0, 2.0, 3.0)
        data[0..4].copy_from_slice(&1.0f32.to_le_bytes());
        data[4..8].copy_from_slice(&2.0f32.to_le_bytes());
        data[8..12].copy_from_slice(&3.0f32.to_le_bytes());
        // scale = (0.1, 0.1, 0.1)
        data[12..16].copy_from_slice(&0.1f32.to_le_bytes());
        data[16..20].copy_from_slice(&0.1f32.to_le_bytes());
        data[20..24].copy_from_slice(&0.1f32.to_le_bytes());
        // color = (255, 128, 0, 200)
        data[24] = 255;
        data[25] = 128;
        data[26] = 0;
        data[27] = 200;
        // rotation = (128, 128, 128, 128) → (0, 0, 0, 0) normalized
        data[28] = 128;
        data[29] = 128;
        data[30] = 128;
        data[31] = 128;

        let cloud = load_splat(&data).unwrap();
        assert_eq!(cloud.count(), 1);
        assert!((cloud.splats[0].position[0] - 1.0).abs() < 0.01);
        assert!((cloud.splats[0].position[1] - 2.0).abs() < 0.01);
        assert!((cloud.splats[0].position[2] - 3.0).abs() < 0.01);
    }

    #[test]
    fn load_ply_ascii() {
        let ply = b"ply\nformat ascii 1.0\nelement vertex 2\nproperty float x\nproperty float y\nproperty float z\nend_header\n1.0 2.0 3.0\n4.0 5.0 6.0\n";
        let cloud = load_ply(ply).unwrap();
        assert_eq!(cloud.count(), 2);
        assert!((cloud.splats[0].position[0] - 1.0).abs() < 0.01);
        assert!((cloud.splats[1].position[0] - 4.0).abs() < 0.01);
    }

    #[test]
    fn load_ply_binary_le_with_non_utf8_body() {
        // Regression: pre-2026-05-09 the parser ran `from_utf8` over the
        // whole payload, which rejected any binary PLY whose body bytes
        // happened to fall outside ASCII (every real 3DGS export). The
        // fix scans raw bytes for `end_header\n` and only validates the
        // header slice as UTF-8.
        let mut hdr = String::from("ply\nformat binary_little_endian 1.0\nelement vertex 1\n");
        for p in [
            "x", "y", "z", "opacity", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2",
            "rot_3", "f_dc_0", "f_dc_1", "f_dc_2",
        ] {
            hdr.push_str(&format!("property float {p}\n"));
        }
        hdr.push_str("end_header\n");
        let mut bytes = hdr.into_bytes();
        // 14 properties × 4 bytes = 56 byte body. Fill with non-ASCII
        // bytes guaranteed to break naive `from_utf8` (0xFF starts no
        // valid sequence).
        let body = [
            // x=1.0, y=2.0, z=3.0, opacity=0.5
            0u8, 0, 128, 63, 0, 0, 0, 64, 0, 0, 64, 64, 0, 0, 0, 63, // scale_0/1/2 = 0.1
            0xCD, 0xCC, 0xCC, 0x3D, 0xCD, 0xCC, 0xCC, 0x3D, 0xCD, 0xCC, 0xCC, 0x3D,
            // rot_0/1/2/3 = 1, 0, 0, 0
            0, 0, 128, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // f_dc_0/1/2 = 0.5
            0, 0, 0, 63, 0, 0, 0, 63, 0, 0, 0, 63,
        ];
        bytes.extend_from_slice(&body);
        let cloud = load_ply(&bytes).expect("binary PLY must parse");
        assert_eq!(cloud.count(), 1);
        let s = &cloud.splats[0];
        assert!((s.position[0] - 1.0).abs() < 1e-4);
        assert!((s.position[1] - 2.0).abs() < 1e-4);
        assert!((s.position[2] - 3.0).abs() < 1e-4);
        assert!((s.opacity - 0.5).abs() < 1e-4);
    }

    #[test]
    fn load_ply_binary_with_f_rest_degree_1() {
        // Degree-1 PLY = 14 base + 9 f_rest_* (= 3 coefs × 3 channels).
        // Layout per splat (channel-major): R0..R2, G0..G2, B0..B2.
        let mut hdr = String::from("ply\nformat binary_little_endian 1.0\nelement vertex 1\n");
        for p in [
            "x", "y", "z", "opacity", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2",
            "rot_3", "f_dc_0", "f_dc_1", "f_dc_2",
        ] {
            hdr.push_str(&format!("property float {p}\n"));
        }
        for i in 0..9 {
            hdr.push_str(&format!("property float f_rest_{i}\n"));
        }
        hdr.push_str("end_header\n");
        let mut bytes = hdr.into_bytes();
        // 14 + 9 = 23 floats × 4 = 92 bytes per splat.
        let mut body: Vec<u8> = Vec::with_capacity(23 * 4);
        // base 14 — geometry / DC, values irrelevant for this test.
        for v in [
            1.0_f32, 2.0, 3.0, 0.5, // pos + opacity
            -2.0, -2.0, -2.0, // log scale
            1.0, 0.0, 0.0, 0.0, // quat
            0.1, 0.2, 0.3, // f_dc
        ] {
            body.extend_from_slice(&v.to_le_bytes());
        }
        // f_rest: R0..R2 = 1,2,3 / G0..G2 = 4,5,6 / B0..B2 = 7,8,9.
        for v in [1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0] {
            body.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&body);

        let cloud = load_ply(&bytes).expect("binary PLY with f_rest must parse");
        assert_eq!(cloud.count(), 1);
        assert_eq!(cloud.sh_degree, 1);
        // 1 splat × (K-1)=3 coefficients = 3 RGB triples.
        assert_eq!(cloud.sh_rest.len(), 3);
        // Coefficient-major rearrange: coef-0 RGB = (R0, G0, B0) = (1, 4, 7)
        assert_eq!(cloud.sh_rest[0], [1.0, 4.0, 7.0]);
        assert_eq!(cloud.sh_rest[1], [2.0, 5.0, 8.0]);
        assert_eq!(cloud.sh_rest[2], [3.0, 6.0, 9.0]);
    }

    #[test]
    fn load_ply_binary_tolerates_truncated_body() {
        // Streaming-LOD design: the dumper emits PLY with the splats
        // sorted opacity-descending, then the browser HTTP-Range-
        // fetches only the first M bytes when the tile is far from the
        // player. The header still announces the *full* vertex count
        // (e.g. 100), but only the first ⌊M/stride⌋ records are
        // present in the body. This test pins the loader behaviour:
        // a 100-splat header + 30 splats' worth of body must yield
        // exactly 30 splats, no panic.
        let mut hdr = String::from("ply\nformat binary_little_endian 1.0\nelement vertex 100\n");
        for p in [
            "x", "y", "z", "opacity", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2",
            "rot_3", "f_dc_0", "f_dc_1", "f_dc_2",
        ] {
            hdr.push_str(&format!("property float {p}\n"));
        }
        hdr.push_str("end_header\n");
        let mut bytes = hdr.into_bytes();
        // 14 properties × 4 = 56 bytes per splat. Write 30 splats with
        // distinct positions (i.e. 1.0..30.0 on x).
        for i in 0..30u32 {
            for v in [
                i as f32, 0.0, 0.0, 0.5, -2.0, -2.0, -2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            ] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        let cloud = load_ply(&bytes).expect("truncated PLY must parse");
        assert_eq!(
            cloud.count(),
            30,
            "loader must short-circuit on partial body, got {}",
            cloud.count()
        );
        assert!((cloud.splats[29].position[0] - 29.0).abs() < 1e-4);
    }

    #[test]
    fn load_ply_with_opacity() {
        let ply = b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nproperty float opacity\nend_header\n1.0 2.0 3.0 0.5\n";
        let cloud = load_ply(ply).unwrap();
        assert_eq!(cloud.count(), 1);
        assert!((cloud.splats[0].opacity - 0.5).abs() < 0.01);
    }
}
