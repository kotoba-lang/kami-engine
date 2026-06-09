//! KTX2 container parsing + `KHR_texture_basisu` **UASTC LDR** transcoding —
//! behind the `gltf-loader` feature.
//!
//! Pure-Rust, WASM-safe. The UASTC 4×4 block decoder is a faithful port of the
//! Basis Universal reference transcoder (`basisu_transcoder.cpp` `unpack_uastc`,
//! BinomialLLC/basis_universal). All lookup tables in `uastc_tables` are
//! auto-generated from that transcoder; the block decoder is validated
//! bit-exactly against encode→decode reference vectors in tests.
//!
//! Scope (per project decision): **UASTC only**. ETC1S (BasisLZ supercompressed)
//! KTX2 textures are detected and reported unsupported (caller substitutes a
//! placeholder). Supercompression: `none` and `ZLIB` are handled; `Zstandard`
//! is reported unsupported (no pure-Rust zstd in the dependency set).

#![cfg(feature = "gltf-loader")]

use crate::uastc_tables as t;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BasisError {
    #[error("not a KTX2 file")]
    NotKtx2,
    #[error("truncated KTX2 data")]
    Truncated,
    #[error("ETC1S/BasisLZ KTX2 textures are not supported (UASTC only)")]
    Etc1sUnsupported,
    #[error("unsupported KTX2 supercompression scheme {0}")]
    UnsupportedSupercompression(u32),
    #[error("invalid UASTC block (mode {0})")]
    BadBlock(u32),
    #[error("zlib decompress failed: {0}")]
    Zlib(String),
}

/// A decoded texture: tightly-packed RGBA8, row-major, top-left origin.
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

// ---------------------------------------------------------------------------
// UASTC 4x4 LDR block decode
// ---------------------------------------------------------------------------

/// LSB-first bit reader over a fixed 16-byte UASTC block.
struct BitReader<'a> {
    bytes: &'a [u8; 16],
    pos: usize, // bit offset
}

impl<'a> BitReader<'a> {
    #[inline]
    fn read(&mut self, count: u32) -> u32 {
        let mut v = 0u32;
        for i in 0..count {
            let byte = self.bytes[self.pos >> 3];
            let bit = (byte >> (self.pos & 7)) & 1;
            v |= (bit as u32) << i;
            self.pos += 1;
        }
        v
    }
}

const UASTC_MODE_SOLID: usize = 8;

/// ASTC endpoint→weight interpolation (Basis `astc_interpolate`, LDR path).
#[inline]
fn astc_interpolate(l: u32, h: u32, w: u32) -> u32 {
    let l = (l << 8) | l;
    let h = (h << 8) | h;
    let k = (l * (64 - w) + h * w + 32) >> 6;
    k >> 8
}

/// Decode a single 16-byte UASTC LDR block into 16 RGBA pixels (raster order).
pub fn decode_uastc_block(block: &[u8; 16]) -> Result<[[u8; 4]; 16], BasisError> {
    let mode = t::HUFF_MODES[(block[0] & 127) as usize] as usize;
    if mode >= 19 {
        return Err(BasisError::BadBlock(mode as u32));
    }

    let mut br = BitReader { bytes: block, pos: 0 };
    br.pos = t::HUFF_CODES[mode][1] as usize; // skip the mode huffman code

    // Solid-color mode.
    if mode == UASTC_MODE_SOLID {
        let r = br.read(8) as u8;
        let g = br.read(8) as u8;
        let b = br.read(8) as u8;
        let a = br.read(8) as u8;
        return Ok([[r, g, b, a]; 16]);
    }

    // Skip the BC1/ETC hint bits (we decode straight to RGBA).
    br.pos += t::MODE_TOTAL_HINT_BITS[mode] as usize;

    // Subsets + common partition pattern.
    let mut subsets = 1usize;
    let mut common_pattern = 0u32;
    match mode {
        2 | 4 | 7 | 9 | 16 => {
            common_pattern = br.read(5);
            subsets = 2;
        }
        3 => {
            common_pattern = br.read(4);
            subsets = 3;
        }
        _ => {}
    }

    // Planes (dual-plane) + colour-component selector.
    let mut total_planes = 1usize;
    let mut ccs = -1i32;
    match mode {
        6 | 11 | 13 => {
            ccs = br.read(2) as i32;
            total_planes = 2;
        }
        17 => {
            ccs = 3;
            total_planes = 2;
        }
        _ => {}
    }

    let total_comps = t::MODE_COMPS[mode] as usize;
    let weight_bits = t::MODE_WEIGHT_BITS[mode] as usize;
    let endpoint_range = t::MODE_ENDPOINT_RANGES[mode] as usize;

    // ---- endpoint BISE decode ----
    let total_values = total_comps * 2 * subsets;
    let ep_bits = t::BISE_RANGE[endpoint_range][0] as u32;
    let ep_trits = t::BISE_RANGE[endpoint_range][1] != 0;
    let ep_quints = t::BISE_RANGE[endpoint_range][2] != 0;

    let (total_tqs, bundle_size, mul) = if ep_trits {
        ((total_values + 4) / 5, 5usize, 3u32)
    } else if ep_quints {
        ((total_values + 2) / 3, 3usize, 5u32)
    } else {
        (0, 0, 0)
    };

    let mut tq_values = [0u32; 8];
    for i in 0..total_tqs {
        let mut num_bits = if ep_trits { 8 } else { 7 };
        if i == total_tqs - 1 {
            let num_remaining = total_values - (total_tqs - 1) * bundle_size;
            if ep_trits {
                num_bits = match num_remaining {
                    1 => 2,
                    2 => 4,
                    3 => 5,
                    4 => 7,
                    _ => num_bits,
                };
            } else if ep_quints {
                num_bits = match num_remaining {
                    1 => 3,
                    2 => 5,
                    _ => num_bits,
                };
            }
        }
        tq_values[i] = br.read(num_bits);
    }

    let mut endpoints = [0u8; 18];
    {
        let mut accum = 0u32;
        let mut accum_remaining = 0usize;
        let mut next_tq = 0usize;
        for ep in endpoints.iter_mut().take(total_values) {
            let mut value = br.read(ep_bits);
            if total_tqs != 0 {
                if accum_remaining == 0 {
                    accum = tq_values[next_tq];
                    next_tq += 1;
                    accum_remaining = bundle_size;
                }
                let v = accum % mul;
                accum /= mul;
                accum_remaining -= 1;
                value |= v << ep_bits;
            }
            *ep = value as u8;
        }
    }

    // ---- partition pattern + subset anchor indices ----
    let zero16 = [0u8; 16];
    let (pattern, anchors): (&[u8], [u8; 3]) = if subsets == 1 {
        (&zero16, [0, 0, 0])
    } else if subsets == 3 {
        let p = common_pattern as usize;
        if p >= t::PATTERNS3.len() {
            return Err(BasisError::BadBlock(mode as u32));
        }
        (&t::PATTERNS3[p], t::PATTERNS3_ANCHORS[p])
    } else if mode == 7 {
        let p = common_pattern as usize;
        if p >= t::PATTERNS2_BC7M3.len() {
            return Err(BasisError::BadBlock(mode as u32));
        }
        (&t::PATTERNS2_BC7M3[p], t::PATTERNS2_BC7M3_ANCHORS[p])
    } else {
        let p = common_pattern as usize;
        if p >= t::PATTERNS2.len() {
            return Err(BasisError::BadBlock(mode as u32));
        }
        (&t::PATTERNS2[p], t::PATTERNS2_ANCHORS[p])
    };

    // ---- weight BISE decode (plain binary; anchors drop the high bit) ----
    let mut weights = [0u8; 64];
    let total_weights = 16 * total_planes;
    if total_planes == 2 {
        // dual plane, single subset: first two weight slots are anchors.
        weights[0] = br.read(weight_bits as u32 - 1) as u8;
        weights[1] = br.read(weight_bits as u32 - 1) as u8;
        for w in weights.iter_mut().take(total_weights).skip(2) {
            *w = br.read(weight_bits as u32) as u8;
        }
    } else if subsets == 1 {
        weights[0] = br.read(weight_bits as u32 - 1) as u8; // anchor
        for w in weights.iter_mut().take(16).skip(1) {
            *w = br.read(weight_bits as u32) as u8;
        }
    } else {
        let (a0, a1, a2) = (anchors[0], anchors[1], anchors[2]);
        for (i, w) in weights.iter_mut().take(16).enumerate() {
            let is_anchor = i as u8 == a0 || (subsets >= 2 && i as u8 == a1) || (subsets >= 3 && i as u8 == a2);
            *w = if is_anchor {
                br.read(weight_bits as u32 - 1) as u8
            } else {
                br.read(weight_bits as u32) as u8
            };
        }
    }

    // ---- unquantize endpoints ----
    // endpoints[subset*comps*2 + comp*2 + {0,1}] are quantized indices.
    let unq = &t::ASTC_UNQUANT[endpoint_range];
    let mut ep_lo = [[0u32; 4]; 3];
    let mut ep_hi = [[0u32; 4]; 3];
    for s in 0..subsets {
        if total_comps == 2 {
            // luminance+alpha: L in rgb, A in w
            let ll = unq[endpoints[s * 4] as usize] as u32;
            let lh = unq[endpoints[s * 4 + 1] as usize] as u32;
            let al = unq[endpoints[s * 4 + 2] as usize] as u32;
            let ah = unq[endpoints[s * 4 + 3] as usize] as u32;
            ep_lo[s] = [ll, ll, ll, al];
            ep_hi[s] = [lh, lh, lh, ah];
        } else {
            for c in 0..total_comps {
                ep_lo[s][c] = unq[endpoints[s * total_comps * 2 + c * 2] as usize] as u32;
                ep_hi[s][c] = unq[endpoints[s * total_comps * 2 + c * 2 + 1] as usize] as u32;
            }
            for c in total_comps..4 {
                ep_lo[s][c] = 255;
                ep_hi[s][c] = 255;
            }
        }
    }

    // ---- precompute per-subset block colors for every weight level ----
    let weight_levels = 1usize << weight_bits;
    let wtab = &t::WEIGHT_TABLES[weight_bits];
    let mut block_colors = [[[0u8; 4]; 32]; 3];
    for s in 0..subsets {
        for l in 0..weight_levels {
            let w = wtab[l] as u32;
            for c in 0..4 {
                block_colors[s][l][c] = astc_interpolate(ep_lo[s][c], ep_hi[s][c], w) as u8;
            }
        }
    }

    // ---- assemble pixels ----
    let mut pixels = [[0u8; 4]; 16];
    if total_planes == 1 {
        for i in 0..16 {
            let s = if subsets == 1 { 0 } else { pattern[i] as usize };
            pixels[i] = block_colors[s][weights[i] as usize];
        }
    } else {
        // dual plane, single subset: ccs component uses the second weight.
        for i in 0..16 {
            let w0 = weights[i * 2] as usize;
            let w1 = weights[i * 2 + 1] as usize;
            for c in 0..4 {
                pixels[i][c] = if c as i32 == ccs {
                    block_colors[0][w1][c]
                } else {
                    block_colors[0][w0][c]
                };
            }
        }
    }

    Ok(pixels)
}

// ---------------------------------------------------------------------------
// KTX2 container parsing
// ---------------------------------------------------------------------------

const KTX2_IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];

// KHR Data Format colour models.
const KHR_DF_MODEL_UASTC: u32 = 166;
const KHR_DF_MODEL_ETC1S: u32 = 163;

#[inline]
fn u32le(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}
#[inline]
fn u64le(d: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?))
}

/// Returns true if `data` begins with the KTX2 file identifier.
pub fn is_ktx2(data: &[u8]) -> bool {
    data.len() >= 12 && data[..12] == KTX2_IDENTIFIER
}

/// Decode the base level (mip 0) of a UASTC KTX2 texture to RGBA8.
pub fn decode_ktx2(data: &[u8]) -> Result<DecodedImage, BasisError> {
    if !is_ktx2(data) {
        return Err(BasisError::NotKtx2);
    }
    // KTX2 header field offsets (the 12-byte identifier precedes them):
    //   12 vkFormat · 16 typeSize · 20 pixelWidth · 24 pixelHeight ·
    //   28 pixelDepth · 32 layerCount · 36 faceCount · 40 levelCount ·
    //   44 supercompressionScheme · then the index (48 dfdByteOffset …).
    let width = u32le(data, 20).ok_or(BasisError::Truncated)?;
    let height = u32le(data, 24).ok_or(BasisError::Truncated)?.max(1);
    let level_count = u32le(data, 40).ok_or(BasisError::Truncated)?.max(1);
    let supercompression = u32le(data, 44).ok_or(BasisError::Truncated)?;
    let dfd_offset = u32le(data, 48).ok_or(BasisError::Truncated)? as usize;

    // Distinguish UASTC from ETC1S. BasisLZ supercompression (scheme 1) is
    // always ETC1S. As a secondary check, read the DFD colour model byte.
    if supercompression == 1 {
        return Err(BasisError::Etc1sUnsupported);
    }
    if dfd_offset != 0 {
        // DFD = dfdTotalSize(u32) then the basic descriptor block. colorModel
        // is the first byte of the block's 3rd u32 → dfd_offset + 4 + 8.
        if let Some(&color_model) = data.get(dfd_offset + 12) {
            if color_model as u32 == KHR_DF_MODEL_ETC1S {
                return Err(BasisError::Etc1sUnsupported);
            }
            let _ = KHR_DF_MODEL_UASTC; // documented; UASTC is the default path
        }
    }

    // Level index: levelCount × (byteOffset u64, byteLength u64, uncompLen u64),
    // starting at offset 80. Level 0 is the largest (base) mip.
    let li = 80usize;
    let lvl_off = u64le(data, li).ok_or(BasisError::Truncated)? as usize;
    let lvl_len = u64le(data, li + 8).ok_or(BasisError::Truncated)? as usize;
    let lvl_uncomp = u64le(data, li + 16).ok_or(BasisError::Truncated)? as usize;
    let _ = level_count;

    let raw = data.get(lvl_off..lvl_off + lvl_len).ok_or(BasisError::Truncated)?;

    // Supercompression of the level data.
    let level_data: Vec<u8> = match supercompression {
        0 => raw.to_vec(),
        3 => {
            // ZLIB
            use std::io::Read;
            let mut dec = flate2::read::ZlibDecoder::new(raw);
            let mut out = Vec::with_capacity(lvl_uncomp.max(raw.len()));
            dec.read_to_end(&mut out).map_err(|e| BasisError::Zlib(e.to_string()))?;
            out
        }
        other => return Err(BasisError::UnsupportedSupercompression(other)),
    };

    // Decode UASTC 4×4 blocks into the RGBA image.
    let bw = width.div_ceil(4);
    let bh = height.div_ceil(4);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut blk = [0u8; 16];
    for by in 0..bh {
        for bx in 0..bw {
            let block_idx = (by * bw + bx) as usize;
            let off = block_idx * 16;
            let src = level_data.get(off..off + 16).ok_or(BasisError::Truncated)?;
            blk.copy_from_slice(src);
            let pixels = decode_uastc_block(&blk)?;
            for py in 0..4 {
                for px in 0..4 {
                    let x = bx * 4 + px;
                    let y = by * 4 + py;
                    if x >= width || y >= height {
                        continue;
                    }
                    let p = pixels[(py * 4 + px) as usize];
                    let di = ((y * width + x) * 4) as usize;
                    rgba[di..di + 4].copy_from_slice(&p);
                }
            }
        }
    }

    Ok(DecodedImage { width, height, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("uastc_vectors.rs");

    #[test]
    fn uastc_blocks_match_reference_decoder() {
        let mut tested = 0;
        for (block, expected) in VECTORS {
            let pixels = decode_uastc_block(block).expect("decode");
            let mut got = [0u8; 64];
            for i in 0..16 {
                got[i * 4..i * 4 + 4].copy_from_slice(&pixels[i]);
            }
            assert_eq!(&got, expected, "block {block:02x?}");
            tested += 1;
        }
        assert!(tested >= 100, "expected many vectors, got {tested}");
    }

    #[test]
    fn ktx2_identifier_detect() {
        assert!(is_ktx2(&KTX2_IDENTIFIER));
        assert!(!is_ktx2(b"not a ktx2 file"));
    }

    /// Build a minimal 8×8 UASTC KTX2 (2×2 blocks, supercompression none) from
    /// four reference blocks and verify decode_ktx2 reconstructs the image —
    /// exercising the container parsing + block iteration + RGBA assembly.
    #[test]
    fn ktx2_container_roundtrip() {
        let blocks: Vec<[u8; 16]> = VECTORS.iter().take(4).map(|(b, _)| *b).collect();
        // Block raster order for an 8×8 (bw=2, bh=2) image: (0,0)(1,0)(0,1)(1,1).
        let mut level = Vec::new();
        for b in &blocks {
            level.extend_from_slice(b);
        }

        let mut k = Vec::new();
        k.extend_from_slice(&KTX2_IDENTIFIER);
        k.extend_from_slice(&0u32.to_le_bytes()); // vkFormat
        k.extend_from_slice(&1u32.to_le_bytes()); // typeSize
        k.extend_from_slice(&8u32.to_le_bytes()); // width
        k.extend_from_slice(&8u32.to_le_bytes()); // height
        k.extend_from_slice(&0u32.to_le_bytes()); // depth
        k.extend_from_slice(&0u32.to_le_bytes()); // layerCount
        k.extend_from_slice(&1u32.to_le_bytes()); // faceCount
        k.extend_from_slice(&1u32.to_le_bytes()); // levelCount
        k.extend_from_slice(&0u32.to_le_bytes()); // supercompressionScheme = none
        // index: dfd off/len, kvd off/len (u32), sgd off/len (u64) — all zero
        k.extend_from_slice(&0u32.to_le_bytes());
        k.extend_from_slice(&0u32.to_le_bytes());
        k.extend_from_slice(&0u32.to_le_bytes());
        k.extend_from_slice(&0u32.to_le_bytes());
        k.extend_from_slice(&0u64.to_le_bytes());
        k.extend_from_slice(&0u64.to_le_bytes());
        // level index (1 level) at offset 80: byteOffset, byteLength, uncompLen
        let level_offset = 80u64 + 24; // header+index = 80, +24 for one level entry
        k.extend_from_slice(&level_offset.to_le_bytes());
        k.extend_from_slice(&(level.len() as u64).to_le_bytes());
        k.extend_from_slice(&(level.len() as u64).to_le_bytes());
        assert_eq!(k.len() as u64, level_offset);
        k.extend_from_slice(&level);

        let img = decode_ktx2(&k).expect("decode ktx2");
        assert_eq!((img.width, img.height), (8, 8));
        assert_eq!(img.rgba.len(), 8 * 8 * 4);

        // Verify each block's pixels landed in the right 4×4 region.
        for (bi, b) in blocks.iter().enumerate() {
            let bx = (bi % 2) as u32;
            let by = (bi / 2) as u32;
            let px = decode_uastc_block(b).unwrap();
            for ty in 0..4u32 {
                for tx in 0..4u32 {
                    let x = bx * 4 + tx;
                    let y = by * 4 + ty;
                    let di = ((y * 8 + x) * 4) as usize;
                    assert_eq!(
                        &img.rgba[di..di + 4],
                        &px[(ty * 4 + tx) as usize],
                        "block {bi} texel ({tx},{ty})"
                    );
                }
            }
        }
    }

    #[test]
    fn ktx2_etc1s_reported_unsupported() {
        let mut k = vec![0u8; 80];
        k[..12].copy_from_slice(&KTX2_IDENTIFIER);
        k[20..24].copy_from_slice(&4u32.to_le_bytes()); // width
        k[24..28].copy_from_slice(&4u32.to_le_bytes()); // height
        k[40..44].copy_from_slice(&1u32.to_le_bytes()); // levelCount
        k[44..48].copy_from_slice(&1u32.to_le_bytes()); // BasisLZ → ETC1S
        assert!(matches!(decode_ktx2(&k), Err(BasisError::Etc1sUnsupported)));
    }
}
