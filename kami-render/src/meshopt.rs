//! Pure-Rust decoders for `EXT_meshopt_compression` (glTF) — behind the
//! `gltf-loader` feature.
//!
//! This is a faithful, scalar port of the canonical meshoptimizer reference
//! decoders (`vertexcodec.cpp`, `indexcodec.cpp`, `vertexfilter.cpp`,
//! zeux/meshoptimizer, MIT). It is byte-for-byte compatible with the format
//! version 0 and 1 bitstreams emitted by `gltfpack` / `meshopt_encode*`.
//!
//! WASM-safe: no SIMD, no C dependency. The reference SIMD paths produce
//! identical output to the scalar path ported here.
//!
//! Public surface:
//!   - [`decode_vertex_buffer`] / [`decode_index_buffer`] / [`decode_index_sequence`]
//!   - [`decode_filter_oct`] / [`decode_filter_quat`] / [`decode_filter_exp`]
//!   - [`decode_meshopt_glb`] — preprocess a GLB: decode all meshopt buffer
//!     views and emit a clean, extension-free GLB the `gltf` crate can import.

#![cfg(feature = "gltf-loader")]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MeshoptError {
    #[error("invalid meshopt vertex stream header")]
    BadVertexHeader,
    #[error("invalid meshopt index stream header")]
    BadIndexHeader,
    #[error("unexpected end of meshopt stream")]
    UnexpectedEof,
    #[error("malformed meshopt stream")]
    Malformed,
    #[error("unsupported meshopt mode/filter: {0}")]
    Unsupported(String),
    #[error("invalid GLB container")]
    BadGlb,
    #[error("glb json error: {0}")]
    Json(String),
}

// ---------------------------------------------------------------------------
// Vertex codec (vertexcodec.cpp)
// ---------------------------------------------------------------------------

const VERTEX_HEADER: u8 = 0xa0;
const DECODE_VERTEX_VERSION: u8 = 1;
const VERTEX_BLOCK_SIZE_BYTES: usize = 8192;
const VERTEX_BLOCK_MAX_SIZE: usize = 256;
const BYTE_GROUP_SIZE: usize = 16;
const BYTE_GROUP_DECODE_LIMIT: usize = 24;
const TAIL_MIN_SIZE_V0: usize = 32;
const TAIL_MIN_SIZE_V1: usize = 24;

const BITS_V0: [i32; 4] = [0, 2, 4, 8];
const BITS_V1: [i32; 5] = [0, 1, 2, 4, 8];

fn vertex_block_size(vertex_size: usize) -> usize {
    let result = (VERTEX_BLOCK_SIZE_BYTES / vertex_size) & !(BYTE_GROUP_SIZE - 1);
    if result < VERTEX_BLOCK_MAX_SIZE {
        result
    } else {
        VERTEX_BLOCK_MAX_SIZE
    }
}

#[inline]
fn unzigzag32(v: u32) -> u32 {
    (0u32.wrapping_sub(v & 1)) ^ (v >> 1)
}

/// Decode one 16-byte byte-group. `pos` indexes into `input`. Returns the new
/// cursor position. Mirrors `decodeBytesGroup` (scalar fallback).
fn decode_bytes_group(input: &[u8], pos: usize, out: &mut [u8], bits: i32) -> Option<usize> {
    match bits {
        0 => {
            for b in out.iter_mut().take(BYTE_GROUP_SIZE) {
                *b = 0;
            }
            Some(pos)
        }
        1 => {
            // 2 header bytes (bit-reversed), then 16 1-bit values; overflow
            // bytes follow at `data_var` (= pos + 2).
            let mut data = pos; // header cursor (READ)
            let mut data_var = pos + 2; // overflow cursor
            let sentinel = (1u16 << 1) - 1; // = 1
            let mut oi = 0;
            for _grp in 0..2 {
                let mut byte = input.get(data).copied()?;
                data += 1;
                byte = byte.reverse_bits();
                for _ in 0..8 {
                    let enc = byte >> 7;
                    byte <<= 1;
                    let encv = input.get(data_var).copied()?;
                    if enc as u16 == sentinel {
                        out[oi] = encv;
                        data_var += 1;
                    } else {
                        out[oi] = enc;
                    }
                    oi += 1;
                }
            }
            let _ = data;
            Some(data_var)
        }
        2 => {
            let mut data = pos;
            let mut data_var = pos + 4;
            let sentinel = (1u8 << 2) - 1; // = 3
            let mut oi = 0;
            for _grp in 0..4 {
                let mut byte = input.get(data).copied()?;
                data += 1;
                for _ in 0..4 {
                    let enc = byte >> 6;
                    byte <<= 2;
                    let encv = input.get(data_var).copied()?;
                    if enc == sentinel {
                        out[oi] = encv;
                        data_var += 1;
                    } else {
                        out[oi] = enc;
                    }
                    oi += 1;
                }
            }
            let _ = data;
            Some(data_var)
        }
        4 => {
            let mut data = pos;
            let mut data_var = pos + 8;
            let sentinel = (1u8 << 4) - 1; // = 15
            let mut oi = 0;
            for _grp in 0..8 {
                let mut byte = input.get(data).copied()?;
                data += 1;
                for _ in 0..2 {
                    let enc = byte >> 4;
                    byte <<= 4;
                    let encv = input.get(data_var).copied()?;
                    if enc == sentinel {
                        out[oi] = encv;
                        data_var += 1;
                    } else {
                        out[oi] = enc;
                    }
                    oi += 1;
                }
            }
            let _ = data;
            Some(data_var)
        }
        8 => {
            let src = input.get(pos..pos + BYTE_GROUP_SIZE)?;
            out[..BYTE_GROUP_SIZE].copy_from_slice(src);
            Some(pos + BYTE_GROUP_SIZE)
        }
        _ => None,
    }
}

/// Mirrors `decodeBytes`: decode `buffer_size` bytes into `out`, reading a
/// 2-bit-per-group header then each group via [`decode_bytes_group`].
/// `bits` is the 4-entry width table selected by the per-group header.
fn decode_bytes(
    input: &[u8],
    mut pos: usize,
    out: &mut [u8],
    buffer_size: usize,
    bits: &[i32],
) -> Option<usize> {
    debug_assert!(buffer_size % BYTE_GROUP_SIZE == 0);
    let header_size = (buffer_size / BYTE_GROUP_SIZE + 3) / 4;
    if input.len().checked_sub(pos)? < header_size {
        return None;
    }
    let header_start = pos;
    pos += header_size;

    let mut i = 0;
    while i < buffer_size {
        if input.len() - pos < BYTE_GROUP_DECODE_LIMIT {
            return None;
        }
        let header_offset = i / BYTE_GROUP_SIZE;
        let bitsk =
            ((input[header_start + header_offset / 4] >> ((header_offset % 4) * 2)) & 3) as usize;
        pos = decode_bytes_group(input, pos, &mut out[i..], bits[bitsk])?;
        i += BYTE_GROUP_SIZE;
    }
    Some(pos)
}

/// Mirrors `decodeDeltas1<T,Xor>`: transpose 4 parallel byte lanes from
/// `buffer` into `transposed` (vertex layout), applying delta+unzigzag (or
/// rotate+xor for channel 2). `n` = component byte width (1,2,4).
#[allow(clippy::too_many_arguments)]
fn decode_deltas1(
    buffer: &[u8],
    transposed: &mut [u8],
    transposed_off: usize,
    vertex_count: usize,
    vertex_size: usize,
    last_vertex: &[u8],
    last_vertex_off: usize,
    n: usize,
    xor: bool,
    rot: u32,
) {
    let mask: u32 = if n >= 4 {
        0xffff_ffff
    } else {
        (1u32 << (8 * n)) - 1
    };
    let mut buf_base = 0usize; // advances by vertex_count*n each outer iter
    let mut lv_off = last_vertex_off; // advances by n
    let mut k = 0usize; // inner byte column, step n
    while k < 4 {
        let mut vertex_offset = transposed_off + k;

        // p = last_vertex[lv_off .. lv_off+n] little-endian
        let mut p: u32 = 0;
        for j in 0..n {
            p |= (last_vertex[lv_off + j] as u32) << (8 * j);
        }

        for i in 0..vertex_count {
            // gather n lanes
            let mut v: u32 = 0;
            for j in 0..n {
                v |= (buffer[buf_base + i + vertex_count * j] as u32) << (8 * j);
            }

            v = if xor {
                (v.rotate_left(rot) ^ p) & mask
            } else {
                (unzigzag32(v).wrapping_add(p)) & mask
            };

            for j in 0..n {
                transposed[vertex_offset + j] = (v >> (j * 8)) as u8;
            }

            p = v;
            vertex_offset += vertex_size;
        }

        buf_base += vertex_count * n;
        lv_off += n;
        k += n;
    }
}

/// Mirrors `decodeVertexBlock`. Decodes one block of up to 256 vertices.
#[allow(clippy::too_many_arguments)]
fn decode_vertex_block(
    input: &[u8],
    mut pos: usize,
    vertex_data: &mut [u8],
    vertex_count: usize,
    vertex_size: usize,
    last_vertex: &mut [u8],
    channels: &[u8],
    version: u8,
) -> Option<usize> {
    debug_assert!(vertex_count > 0 && vertex_count <= VERTEX_BLOCK_MAX_SIZE);

    let mut buffer = [0u8; VERTEX_BLOCK_MAX_SIZE * 4];
    let mut transposed = [0u8; VERTEX_BLOCK_SIZE_BYTES];

    let vertex_count_aligned = (vertex_count + BYTE_GROUP_SIZE - 1) & !(BYTE_GROUP_SIZE - 1);

    let control_size = if version == 0 { 0 } else { vertex_size / 4 };
    if input.len().checked_sub(pos)? < control_size {
        return None;
    }
    let control_start = pos;
    pos += control_size;

    let mut k = 0usize;
    while k < vertex_size {
        let ctrl_byte = if version == 0 {
            0
        } else {
            input[control_start + k / 4]
        };

        for j in 0..4 {
            let ctrl = (ctrl_byte >> (j * 2)) & 3;
            let lane = j * vertex_count;

            if ctrl == 3 {
                // literal encoding
                if input.len() - pos < vertex_count {
                    return None;
                }
                buffer[lane..lane + vertex_count].copy_from_slice(&input[pos..pos + vertex_count]);
                pos += vertex_count;
            } else if ctrl == 2 {
                // zero encoding
                for b in &mut buffer[lane..lane + vertex_count] {
                    *b = 0;
                }
            } else {
                let bits: &[i32] = if version == 0 {
                    &BITS_V0
                } else {
                    &BITS_V1[ctrl as usize..]
                };
                pos = decode_bytes(input, pos, &mut buffer[lane..], vertex_count_aligned, bits)?;
            }
        }

        let channel = if version == 0 { 0 } else { channels[k / 4] };
        match channel & 3 {
            0 => decode_deltas1(
                &buffer,
                &mut transposed,
                k,
                vertex_count,
                vertex_size,
                last_vertex,
                k,
                1,
                false,
                0,
            ),
            1 => decode_deltas1(
                &buffer,
                &mut transposed,
                k,
                vertex_count,
                vertex_size,
                last_vertex,
                k,
                2,
                false,
                0,
            ),
            2 => {
                let rot = (32u32.wrapping_sub((channel >> 4) as u32)) & 31;
                decode_deltas1(
                    &buffer,
                    &mut transposed,
                    k,
                    vertex_count,
                    vertex_size,
                    last_vertex,
                    k,
                    4,
                    true,
                    rot,
                );
            }
            _ => return None,
        }

        k += 4;
    }

    let total = vertex_count * vertex_size;
    vertex_data[..total].copy_from_slice(&transposed[..total]);
    last_vertex[..vertex_size]
        .copy_from_slice(&transposed[vertex_size * (vertex_count - 1)..vertex_size * vertex_count]);

    Some(pos)
}

/// Decode a meshopt-compressed vertex buffer (`mode = ATTRIBUTES`).
/// `out` must be exactly `vertex_count * vertex_size` bytes.
pub fn decode_vertex_buffer(
    out: &mut [u8],
    vertex_count: usize,
    vertex_size: usize,
    data: &[u8],
) -> Result<(), MeshoptError> {
    if vertex_size == 0 || vertex_size > 256 || vertex_size % 4 != 0 {
        return Err(MeshoptError::Unsupported(format!(
            "vertex_size={vertex_size}"
        )));
    }
    if data.is_empty() {
        return Err(MeshoptError::UnexpectedEof);
    }
    let header = data[0];
    if header & 0xf0 != VERTEX_HEADER {
        return Err(MeshoptError::BadVertexHeader);
    }
    let version = header & 0x0f;
    if version > DECODE_VERTEX_VERSION {
        return Err(MeshoptError::Unsupported(format!(
            "vertex version {version}"
        )));
    }
    let mut pos = 1usize;

    let tail_size = vertex_size + if version == 0 { 0 } else { vertex_size / 4 };
    let tail_size_min = if version == 0 {
        TAIL_MIN_SIZE_V0
    } else {
        TAIL_MIN_SIZE_V1
    };
    let tail_size_pad = tail_size.max(tail_size_min);
    if data.len() - pos < tail_size_pad {
        return Err(MeshoptError::UnexpectedEof);
    }

    let tail = data.len() - tail_size;
    let mut last_vertex = [0u8; 256];
    last_vertex[..vertex_size].copy_from_slice(&data[tail..tail + vertex_size]);

    // channels table (version >= 1) lives right after last_vertex in the tail.
    let channels: Vec<u8> = if version == 0 {
        Vec::new()
    } else {
        data[tail + vertex_size..tail + vertex_size + vertex_size / 4].to_vec()
    };

    let block = vertex_block_size(vertex_size);
    let mut vertex_offset = 0usize;
    while vertex_offset < vertex_count {
        let block_size = if vertex_offset + block < vertex_count {
            block
        } else {
            vertex_count - vertex_offset
        };
        pos = decode_vertex_block(
            data,
            pos,
            &mut out[vertex_offset * vertex_size..],
            block_size,
            vertex_size,
            &mut last_vertex,
            &channels,
            version,
        )
        .ok_or(MeshoptError::Malformed)?;
        vertex_offset += block_size;
    }

    if data.len() - pos != tail_size_pad {
        return Err(MeshoptError::Malformed);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Index codec (indexcodec.cpp)
// ---------------------------------------------------------------------------

const INDEX_HEADER: u8 = 0xe0;
const SEQUENCE_HEADER: u8 = 0xd0;
const DECODE_INDEX_VERSION: u8 = 1;

#[inline]
fn decode_vbyte(data: &[u8], pos: &mut usize) -> u32 {
    let lead = data[*pos];
    *pos += 1;
    if lead < 128 {
        return lead as u32;
    }
    let mut result = (lead & 127) as u32;
    let mut shift = 7;
    for _ in 0..4 {
        let group = data[*pos];
        *pos += 1;
        result |= ((group & 127) as u32) << shift;
        shift += 7;
        if group < 128 {
            break;
        }
    }
    result
}

#[inline]
fn decode_index(data: &[u8], pos: &mut usize, last: u32) -> u32 {
    let v = decode_vbyte(data, pos);
    let d = (v >> 1) ^ (0u32.wrapping_sub(v & 1));
    last.wrapping_add(d)
}

#[inline]
fn write_index(out: &mut [u8], offset: usize, index_size: usize, value: u32) {
    if index_size == 2 {
        let b = (value as u16).to_le_bytes();
        out[offset * 2] = b[0];
        out[offset * 2 + 1] = b[1];
    } else {
        let b = value.to_le_bytes();
        out[offset * 4..offset * 4 + 4].copy_from_slice(&b);
    }
}

#[inline]
fn push_edge_fifo(fifo: &mut [[u32; 2]; 16], a: u32, b: u32, offset: &mut usize) {
    fifo[*offset][0] = a;
    fifo[*offset][1] = b;
    *offset = (*offset + 1) & 15;
}

#[inline]
fn push_vertex_fifo(fifo: &mut [u32; 16], v: u32, offset: &mut usize, cond: usize) {
    fifo[*offset] = v;
    *offset = (*offset + cond) & 15;
}

/// Decode a meshopt-compressed triangle index buffer (`mode = TRIANGLES`).
/// `out` must be `index_count * index_size` bytes; `index_count % 3 == 0`.
pub fn decode_index_buffer(
    out: &mut [u8],
    index_count: usize,
    index_size: usize,
    buffer: &[u8],
) -> Result<(), MeshoptError> {
    debug_assert!(index_count % 3 == 0);
    if index_size != 2 && index_size != 4 {
        return Err(MeshoptError::Unsupported(format!(
            "index_size={index_size}"
        )));
    }
    if buffer.len() < 1 + index_count / 3 + 16 {
        return Err(MeshoptError::UnexpectedEof);
    }
    if buffer[0] & 0xf0 != INDEX_HEADER {
        return Err(MeshoptError::BadIndexHeader);
    }
    let version = buffer[0] & 0x0f;
    if version > DECODE_INDEX_VERSION {
        return Err(MeshoptError::Unsupported(format!(
            "index version {version}"
        )));
    }

    let mut edgefifo = [[!0u32; 2]; 16];
    let mut vertexfifo = [!0u32; 16];
    let mut edgefifooffset = 0usize;
    let mut vertexfifooffset = 0usize;

    let mut next: u32 = 0;
    let mut last: u32 = 0;
    let fecmax = if version >= 1 { 13 } else { 15 };

    let mut code = 1usize; // code stream cursor
    let mut data = 1 + index_count / 3; // data stream cursor
    let data_safe_end = buffer.len() - 16;

    let mut i = 0;
    while i < index_count {
        if data > data_safe_end {
            return Err(MeshoptError::Malformed);
        }
        let codetri = buffer[code];
        code += 1;

        if codetri < 0xf0 {
            let fe = (codetri >> 4) as usize;
            let a = edgefifo[(edgefifooffset.wrapping_sub(1 + fe)) & 15][0];
            let b = edgefifo[(edgefifooffset.wrapping_sub(1 + fe)) & 15][1];
            let c;

            let fec = (codetri & 15) as i32;
            if fec < fecmax {
                let cf = vertexfifo[(vertexfifooffset.wrapping_sub(1 + fec as usize)) & 15];
                c = if fec == 0 { next } else { cf };
                let fec0 = (fec == 0) as u32;
                next += fec0;
                push_vertex_fifo(&mut vertexfifo, c, &mut vertexfifooffset, fec0 as usize);
            } else {
                c = if fec != 15 {
                    // 13,14 → -1,+1 via fec - (fec ^ 3)
                    last = (last as i32).wrapping_add(fec - (fec ^ 3)) as u32;
                    last
                } else {
                    last = decode_index(buffer, &mut data, last);
                    last
                };
                push_vertex_fifo(&mut vertexfifo, c, &mut vertexfifooffset, 1);
            }

            push_edge_fifo(&mut edgefifo, c, b, &mut edgefifooffset);
            push_edge_fifo(&mut edgefifo, a, c, &mut edgefifooffset);
            write_triangle(out, i, index_size, a, b, c);
        } else if codetri < 0xfe {
            // fast path: codeaux from table at the very end of the buffer
            let codeaux = buffer[data_safe_end + (codetri & 15) as usize];
            let feb = (codeaux >> 4) as usize;
            let fec = (codeaux & 15) as usize;

            let a = next;
            next += 1;

            let bf = vertexfifo[(vertexfifooffset.wrapping_sub(feb)) & 15];
            let b = if feb == 0 { next } else { bf };
            let feb0 = (feb == 0) as u32;
            next += feb0;

            let cf = vertexfifo[(vertexfifooffset.wrapping_sub(fec)) & 15];
            let c = if fec == 0 { next } else { cf };
            let fec0 = (fec == 0) as u32;
            next += fec0;

            write_triangle(out, i, index_size, a, b, c);

            push_vertex_fifo(&mut vertexfifo, a, &mut vertexfifooffset, 1);
            push_vertex_fifo(&mut vertexfifo, b, &mut vertexfifooffset, feb0 as usize);
            push_vertex_fifo(&mut vertexfifo, c, &mut vertexfifooffset, fec0 as usize);

            push_edge_fifo(&mut edgefifo, b, a, &mut edgefifooffset);
            push_edge_fifo(&mut edgefifo, c, b, &mut edgefifooffset);
            push_edge_fifo(&mut edgefifo, a, c, &mut edgefifooffset);
        } else {
            // slow path: full byte for codeaux
            let codeaux = buffer[data];
            data += 1;

            let fea = if codetri == 0xfe { 0usize } else { 15usize };
            let feb = (codeaux >> 4) as usize;
            let fec = (codeaux & 15) as usize;

            if codeaux == 0 {
                next = 0;
            }

            let mut a = if fea == 0 {
                let t = next;
                next += 1;
                t
            } else {
                0
            };
            let mut b = if feb == 0 {
                let t = next;
                next += 1;
                t
            } else {
                vertexfifo[(vertexfifooffset.wrapping_sub(feb)) & 15]
            };
            let mut c = if fec == 0 {
                let t = next;
                next += 1;
                t
            } else {
                vertexfifo[(vertexfifooffset.wrapping_sub(fec)) & 15]
            };

            if fea == 15 {
                last = decode_index(buffer, &mut data, last);
                a = last;
            }
            if feb == 15 {
                last = decode_index(buffer, &mut data, last);
                b = last;
            }
            if fec == 15 {
                last = decode_index(buffer, &mut data, last);
                c = last;
            }

            write_triangle(out, i, index_size, a, b, c);

            push_vertex_fifo(&mut vertexfifo, a, &mut vertexfifooffset, 1);
            push_vertex_fifo(
                &mut vertexfifo,
                b,
                &mut vertexfifooffset,
                ((feb == 0) || (feb == 15)) as usize,
            );
            push_vertex_fifo(
                &mut vertexfifo,
                c,
                &mut vertexfifooffset,
                ((fec == 0) || (fec == 15)) as usize,
            );

            push_edge_fifo(&mut edgefifo, b, a, &mut edgefifooffset);
            push_edge_fifo(&mut edgefifo, c, b, &mut edgefifooffset);
            push_edge_fifo(&mut edgefifo, a, c, &mut edgefifooffset);
        }

        i += 3;
    }

    if data != data_safe_end {
        return Err(MeshoptError::Malformed);
    }
    Ok(())
}

#[inline]
fn write_triangle(out: &mut [u8], i: usize, index_size: usize, a: u32, b: u32, c: u32) {
    write_index(out, i, index_size, a);
    write_index(out, i + 1, index_size, b);
    write_index(out, i + 2, index_size, c);
}

/// Decode a meshopt-compressed index sequence (`mode = INDICES`).
pub fn decode_index_sequence(
    out: &mut [u8],
    index_count: usize,
    index_size: usize,
    buffer: &[u8],
) -> Result<(), MeshoptError> {
    if index_size != 2 && index_size != 4 {
        return Err(MeshoptError::Unsupported(format!(
            "index_size={index_size}"
        )));
    }
    if buffer.len() < 1 + index_count + 4 {
        return Err(MeshoptError::UnexpectedEof);
    }
    if buffer[0] & 0xf0 != SEQUENCE_HEADER {
        return Err(MeshoptError::BadIndexHeader);
    }
    let version = buffer[0] & 0x0f;
    if version > DECODE_INDEX_VERSION {
        return Err(MeshoptError::Unsupported(format!("seq version {version}")));
    }

    let mut data = 1usize;
    let data_safe_end = buffer.len() - 4;
    let mut last = [0u32; 2];

    for i in 0..index_count {
        if data >= data_safe_end {
            return Err(MeshoptError::Malformed);
        }
        let v = decode_vbyte(buffer, &mut data);
        let current = (v & 1) as usize;
        let v = v >> 1;
        let d = (v >> 1) ^ (0u32.wrapping_sub(v & 1));
        let index = last[current].wrapping_add(d);
        last[current] = index;
        write_index(out, i, index_size, index);
    }

    if data != data_safe_end {
        return Err(MeshoptError::Malformed);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Vertex filters (vertexfilter.cpp) — applied after vertex decode
// ---------------------------------------------------------------------------

#[inline]
fn round_f2i(x: f32) -> i32 {
    // C `int(x + (x>=0 ? 0.5 : -0.5))` — round half away from zero, trunc.
    (x + if x >= 0.0 { 0.5 } else { -0.5 }) as i32
}

/// `OCTAHEDRAL` filter. stride 4 → i8 components, stride 8 → i16 components.
pub fn decode_filter_oct(data: &mut [u8], count: usize, stride: usize) {
    if stride == 4 {
        let max = 127.0f32;
        for i in 0..count {
            let base = i * 4;
            let x0 = data[base] as i8 as f32;
            let y0 = data[base + 1] as i8 as f32;
            let z0 = data[base + 2] as i8 as f32 - x0.abs() - y0.abs();
            let t = if z0 >= 0.0 { 0.0 } else { z0 };
            let x = x0 + if x0 >= 0.0 { t } else { -t };
            let y = y0 + if y0 >= 0.0 { t } else { -t };
            let l = (x * x + y * y + z0 * z0).sqrt();
            let s = max / l;
            data[base] = round_f2i(x * s) as i8 as u8;
            data[base + 1] = round_f2i(y * s) as i8 as u8;
            data[base + 2] = round_f2i(z0 * s) as i8 as u8;
        }
    } else {
        let max = 32767.0f32;
        let rd = |d: &[u8], o: usize| -> f32 { i16::from_le_bytes([d[o], d[o + 1]]) as f32 };
        for i in 0..count {
            let base = i * 8;
            let x0 = rd(data, base);
            let y0 = rd(data, base + 2);
            let z0 = rd(data, base + 4) - x0.abs() - y0.abs();
            let t = if z0 >= 0.0 { 0.0 } else { z0 };
            let x = x0 + if x0 >= 0.0 { t } else { -t };
            let y = y0 + if y0 >= 0.0 { t } else { -t };
            let l = (x * x + y * y + z0 * z0).sqrt();
            let s = max / l;
            let w = |d: &mut [u8], o: usize, v: f32| {
                let b = (round_f2i(v) as i16).to_le_bytes();
                d[o] = b[0];
                d[o + 1] = b[1];
            };
            w(data, base, x * s);
            w(data, base + 2, y * s);
            w(data, base + 4, z0 * s);
        }
    }
}

/// `QUATERNION` filter. stride must be 8 (4 × i16).
pub fn decode_filter_quat(data: &mut [u8], count: usize, _stride: usize) {
    let scale = 32767.0f32 / 2.0f32.sqrt();
    let rd = |d: &[u8], o: usize| -> i32 { i16::from_le_bytes([d[o], d[o + 1]]) as i32 };
    for i in 0..count {
        let base = i * 8;
        let c3 = rd(data, base + 6); // data[i*4+3]
        let sf = c3 | 3;
        let s = sf as f32;
        let x = rd(data, base) as f32;
        let y = rd(data, base + 2) as f32;
        let z = rd(data, base + 4) as f32;
        let ws = s * s;
        let ww = ws * 2.0 - x * x - y * y - z * z;
        let w = if ww >= 0.0 { ww } else { 0.0 }.sqrt();
        let ss = scale / s;
        let xf = round_f2i(x * ss);
        let yf = round_f2i(y * ss);
        let zf = round_f2i(z * ss);
        let wf = (w * ss + 0.5) as i32;
        let qc = (c3 & 3) as usize;
        let put = |d: &mut [u8], comp: usize, v: i32| {
            let o = base + comp * 2;
            let b = (v as i16).to_le_bytes();
            d[o] = b[0];
            d[o + 1] = b[1];
        };
        put(data, (qc + 1) & 3, xf);
        put(data, (qc + 2) & 3, yf);
        put(data, (qc + 3) & 3, zf);
        put(data, qc & 3, wf);
    }
}

/// `EXPONENTIAL` filter. Decodes shared-exponent fixed point into f32 bits.
/// `stride` must be a multiple of 4; operates on `count * stride/4` u32 words.
pub fn decode_filter_exp(data: &mut [u8], count: usize, stride: usize) {
    let words = count * (stride / 4);
    for i in 0..words {
        let o = i * 4;
        let v = u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
        let m = ((v << 8) as i32) >> 8; // sign-extend 24-bit mantissa
        let e = (v as i32) >> 24; // signed exponent
        let ui = ((e + 127) as u32) << 23;
        let f = f32::from_bits(ui) * (m as f32);
        data[o..o + 4].copy_from_slice(&f.to_bits().to_le_bytes());
    }
}

// ---------------------------------------------------------------------------
// GLB preprocessing: decode EXT_meshopt_compression buffer views and emit a
// clean GLB the `gltf` crate can import unmodified.
// ---------------------------------------------------------------------------

const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF"
const GLB_CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
const GLB_CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"
const EXT_NAME: &str = "EXT_meshopt_compression";

fn rd_u32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Minimal standard-alphabet base64 decode (ignores whitespace, tolerant of
/// missing padding). Used for `data:` buffer URIs.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let v = val(c)?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Resolve a buffer URI (only `data:` URIs are supported here; external files
/// return `None`, signalling the caller to fall back).
fn resolve_uri(uri: &str) -> Option<Vec<u8>> {
    if let Some(comma) = uri.find(',') {
        if uri[..comma].starts_with("data:") {
            return base64_decode(&uri[comma + 1..]);
        }
    }
    None
}

/// If `data` is a GLB using `EXT_meshopt_compression`, decode every compressed
/// buffer view and return a re-serialized, extension-free GLB. Returns
/// `Ok(None)` when the input is not GLB or does not use the extension (the
/// caller should then use the original bytes), or on an unsupported layout
/// (e.g. external buffer URIs) so loading can still be attempted.
pub fn decode_meshopt_glb(data: &[u8]) -> Result<Option<Vec<u8>>, MeshoptError> {
    use serde_json::Value;

    // --- parse GLB container ---
    if data.len() < 12 || rd_u32(data, 0) != Some(GLB_MAGIC) {
        return Ok(None); // not GLB (could be a .gltf JSON — out of scope here)
    }
    let mut pos = 12usize;
    let mut json_bytes: Option<&[u8]> = None;
    let mut bin_chunk: Option<Vec<u8>> = None;
    while pos + 8 <= data.len() {
        let clen = rd_u32(data, pos).ok_or(MeshoptError::BadGlb)? as usize;
        let ctype = rd_u32(data, pos + 4).ok_or(MeshoptError::BadGlb)?;
        let cstart = pos + 8;
        let cend = cstart.checked_add(clen).ok_or(MeshoptError::BadGlb)?;
        if cend > data.len() {
            return Err(MeshoptError::BadGlb);
        }
        match ctype {
            GLB_CHUNK_JSON => json_bytes = Some(&data[cstart..cend]),
            GLB_CHUNK_BIN => bin_chunk = Some(data[cstart..cend].to_vec()),
            _ => {}
        }
        pos = cend + ((4 - (cend & 3)) & 3); // chunks are 4-byte aligned
    }
    let json_bytes = json_bytes.ok_or(MeshoptError::BadGlb)?;

    let mut root: Value =
        serde_json::from_slice(json_bytes).map_err(|e| MeshoptError::Json(e.to_string()))?;

    // --- bail out early if the extension isn't used ---
    let uses_ext = |key: &str| {
        root.get(key)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().any(|e| e.as_str() == Some(EXT_NAME)))
            .unwrap_or(false)
    };
    if !uses_ext("extensionsUsed") && !uses_ext("extensionsRequired") {
        return Ok(None);
    }

    // --- resolve every buffer to raw bytes ---
    let buffers_json = root
        .get("buffers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut buffers: Vec<Vec<u8>> = Vec::with_capacity(buffers_json.len());
    let mut bin_taken = false;
    for buf in &buffers_json {
        let byte_length = buf.get("byteLength").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let is_fallback = buf
            .pointer("/extensions/EXT_meshopt_compression/fallback")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if let Some(uri) = buf.get("uri").and_then(|v| v.as_str()) {
            match resolve_uri(uri) {
                Some(bytes) => buffers.push(bytes),
                None => return Ok(None), // external uri — let the caller try as-is
            }
        } else if is_fallback {
            buffers.push(vec![0u8; byte_length]);
        } else if !bin_taken {
            let bin = bin_chunk.clone().ok_or(MeshoptError::BadGlb)?;
            buffers.push(bin);
            bin_taken = true;
        } else {
            buffers.push(vec![0u8; byte_length]);
        }
    }

    // --- rebuild every buffer view into a single fresh buffer ---
    let views_json = root
        .get("bufferViews")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut new_bin: Vec<u8> = Vec::new();
    let mut new_views: Vec<Value> = Vec::with_capacity(views_json.len());

    for view in &views_json {
        let mut view = view.clone();
        let ext = view.pointer("/extensions/EXT_meshopt_compression").cloned();

        let bytes: Vec<u8> = if let Some(ext) = ext {
            // compressed view → decode
            let src_buf = ext.get("buffer").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let src_off = ext.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let src_len = ext.get("byteLength").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let stride = ext.get("byteStride").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let count = ext.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let mode = ext
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("ATTRIBUTES");
            let filter = ext.get("filter").and_then(|v| v.as_str()).unwrap_or("NONE");

            let src = buffers
                .get(src_buf)
                .and_then(|b| b.get(src_off..src_off + src_len))
                .ok_or(MeshoptError::Malformed)?;

            let mut out = vec![0u8; stride * count];
            match mode {
                "ATTRIBUTES" => {
                    decode_vertex_buffer(&mut out, count, stride, src)?;
                    match filter {
                        "NONE" => {}
                        "OCTAHEDRAL" => decode_filter_oct(&mut out, count, stride),
                        "QUATERNION" => decode_filter_quat(&mut out, count, stride),
                        "EXPONENTIAL" => decode_filter_exp(&mut out, count, stride),
                        other => return Err(MeshoptError::Unsupported(other.to_string())),
                    }
                }
                "TRIANGLES" => decode_index_buffer(&mut out, count, stride, src)?,
                "INDICES" => decode_index_sequence(&mut out, count, stride, src)?,
                other => return Err(MeshoptError::Unsupported(other.to_string())),
            }
            out
        } else {
            // uncompressed view → copy from its resolved buffer
            let b = view.get("buffer").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let off = view.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let len = view.get("byteLength").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            buffers
                .get(b)
                .and_then(|buf| buf.get(off..off + len))
                .ok_or(MeshoptError::Malformed)?
                .to_vec()
        };

        // place into the new buffer, 4-byte aligned
        while new_bin.len() % 4 != 0 {
            new_bin.push(0);
        }
        let new_off = new_bin.len();
        let new_len = bytes.len();
        new_bin.extend_from_slice(&bytes);

        // rewrite the view: point at the new buffer, drop the extension
        let obj = view.as_object_mut().ok_or(MeshoptError::Malformed)?;
        obj.insert("buffer".into(), Value::from(0u64));
        obj.insert("byteOffset".into(), Value::from(new_off as u64));
        obj.insert("byteLength".into(), Value::from(new_len as u64));
        if let Some(exts) = obj.get_mut("extensions").and_then(|v| v.as_object_mut()) {
            exts.remove(EXT_NAME);
        }
        if obj
            .get("extensions")
            .and_then(|v| v.as_object())
            .map(|m| m.is_empty())
            .unwrap_or(false)
        {
            obj.remove("extensions");
        }
        new_views.push(view);
    }

    // --- patch the document: single buffer, no meshopt extension ---
    let root_obj = root.as_object_mut().ok_or(MeshoptError::Malformed)?;
    root_obj.insert("bufferViews".into(), Value::Array(new_views));
    root_obj.insert(
        "buffers".into(),
        Value::Array(vec![serde_json::json!({ "byteLength": new_bin.len() })]),
    );
    for key in ["extensionsUsed", "extensionsRequired"] {
        if let Some(arr) = root_obj.get_mut(key).and_then(|v| v.as_array_mut()) {
            arr.retain(|e| e.as_str() != Some(EXT_NAME));
            if arr.is_empty() {
                root_obj.remove(key);
            }
        }
    }

    // --- re-serialize as GLB ---
    let mut json_out = serde_json::to_vec(&root).map_err(|e| MeshoptError::Json(e.to_string()))?;
    while json_out.len() % 4 != 0 {
        json_out.push(b' '); // JSON chunk padded with spaces
    }
    while new_bin.len() % 4 != 0 {
        new_bin.push(0); // BIN chunk padded with zeros
    }

    let total = 12 + 8 + json_out.len() + 8 + new_bin.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    glb.extend_from_slice(&2u32.to_le_bytes()); // version
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_out.len() as u32).to_le_bytes());
    glb.extend_from_slice(&GLB_CHUNK_JSON.to_le_bytes());
    glb.extend_from_slice(&json_out);
    glb.extend_from_slice(&(new_bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(&GLB_CHUNK_BIN.to_le_bytes());
    glb.extend_from_slice(&new_bin);

    Ok(Some(glb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vbyte_roundtrip() {
        // single-byte
        let mut p = 0;
        assert_eq!(decode_vbyte(&[5], &mut p), 5);
        // multi-byte: 300 = 0b100101100 → 0xac 0x02
        let mut p = 0;
        assert_eq!(decode_vbyte(&[0xac, 0x02], &mut p), 300);
        assert_eq!(p, 2);
    }

    #[test]
    fn exp_filter_decodes_one() {
        // m=1, e=0 → ldexp(1,0)=1.0
        let mut buf = 0u32.to_le_bytes().to_vec();
        buf[0] = 1; // mantissa low byte = 1, exponent byte = 0
        decode_filter_exp(&mut buf, 1, 4);
        let f = f32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        assert!((f - 1.0).abs() < 1e-6, "got {f}");
    }

    // The following byte vectors were produced by the *real* meshoptimizer
    // C++ encoder (zeux/meshoptimizer, encode version 1) and are decoded here
    // to prove the pure-Rust port is bit-exact. See /tmp generation script in
    // the implementation notes.

    #[test]
    fn vertex_buffer_matches_reference_encoder() {
        let venc: &[u8] = &[
            0xa1, 0xff, 0xff, 0xff, 0xff, 0x00, 0x98, 0x98, 0x98, 0xd8, 0x58, 0x00, 0xa8, 0xa8,
            0xa8, 0x68, 0xe8, 0x00, 0x98, 0xb8, 0x98, 0x78, 0xd8, 0x00, 0x98, 0x98, 0x98, 0x98,
            0xd8, 0x00, 0xa8, 0xa8, 0xa8, 0xa8, 0x68, 0x00, 0x98, 0xb8, 0x98, 0xb8, 0x58, 0x00,
            0xa8, 0x88, 0xa8, 0xc8, 0x68, 0x00, 0xa8, 0xa8, 0xa8, 0x68, 0xe8, 0x00, 0x98, 0xb8,
            0x98, 0x78, 0xd8, 0x00, 0xa8, 0x88, 0xa8, 0x88, 0xe8, 0x00, 0x98, 0x98, 0x98, 0x98,
            0xd8, 0x00, 0x98, 0xb8, 0x98, 0xb8, 0x58, 0x00, 0xa8, 0x88, 0xa8, 0xc8, 0x68, 0x00,
            0x98, 0x98, 0x98, 0xd8, 0x58, 0x00, 0xa8, 0xa8, 0xa8, 0x68, 0xe8, 0x00, 0xa8, 0x88,
            0xa8, 0x88, 0xe8, 0x00, 0x00, 0x00, 0x00, 0x0d, 0x32, 0x57, 0x7c, 0xa0, 0xc7, 0xea,
            0x11, 0x37, 0x58, 0x7d, 0xa6, 0xca, 0xed, 0x10, 0x3b, 0x00, 0x00, 0x00, 0x00,
        ];
        let vexp: &[u8] = &[
            0x0d, 0x32, 0x57, 0x7c, 0xa0, 0xc7, 0xea, 0x11, 0x37, 0x58, 0x7d, 0xa6, 0xca, 0xed,
            0x10, 0x3b, 0x59, 0x86, 0xa3, 0xc8, 0xf4, 0x13, 0x3e, 0x65, 0x83, 0xac, 0xc9, 0xf2,
            0x1e, 0x39, 0x64, 0x8f, 0xa5, 0xda, 0xff, 0x14, 0x48, 0x6f, 0x82, 0xb9, 0xdf, 0xf0,
            0x15, 0x4e, 0x62, 0x85, 0xb8, 0xd3, 0xf1, 0x2e, 0x4b, 0x60, 0x9c, 0xbb, 0xd6, 0x0d,
            0x2b, 0x44, 0x61, 0x9a, 0xb6, 0xd1, 0x0c, 0x27, 0x5d, 0x62, 0x87, 0xac, 0xf0, 0x17,
            0x3a, 0x41, 0x67, 0x88, 0xad, 0xf6, 0x1a, 0x3d, 0x40, 0x6b, 0x89, 0xd6, 0xf3, 0x18,
            0x24, 0x43, 0x6e, 0xb5, 0xd3, 0xfc, 0x19, 0x22, 0x4e, 0x69, 0xb4, 0xdf,
        ];
        let mut out = vec![0u8; vexp.len()];
        decode_vertex_buffer(&mut out, 6, 16, venc).unwrap();
        assert_eq!(out, vexp);
    }

    #[test]
    fn index_triangles_match_reference_encoder() {
        let ienc: &[u8] = &[
            0xe1, 0xf0, 0x10, 0xfe, 0xff, 0x1e, 0xa6, 0xf0, 0x0c, 0xff, 0x02, 0x02, 0x02, 0x00,
            0x76, 0x87, 0x56, 0x67, 0x78, 0xa9, 0x86, 0x65, 0x89, 0x68, 0x98, 0x01, 0x69, 0x00,
            0x00,
        ];
        let iexp: [u32; 18] = [0, 1, 2, 2, 1, 3, 4, 6, 5, 7, 8, 9, 9, 8, 10, 0, 2, 4];
        let mut out = vec![0u8; iexp.len() * 4];
        decode_index_buffer(&mut out, iexp.len(), 4, ienc).unwrap();
        let got: Vec<u32> = out
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(got, iexp);
    }

    #[test]
    fn index_sequence_matches_reference_encoder() {
        let senc: &[u8] = &[
            0xd1, 0x0c, 0x06, 0x0c, 0x0a, 0x10, 0x10, 0x1a, 0x10, 0x02, 0x06, 0x08, 0x00, 0x00,
            0x00, 0x00,
        ];
        let sexp: [u32; 11] = [3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5];
        let mut out = vec![0u8; sexp.len() * 4];
        decode_index_sequence(&mut out, sexp.len(), 4, senc).unwrap();
        let got: Vec<u32> = out
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(got, sexp);
    }

    #[test]
    fn filter_oct_matches_reference() {
        let mut buf: Vec<u8> = vec![
            0x00, 0x00, 0x7f, 0x7f, 0x00, 0x7f, 0x7f, 0x81, 0x2a, 0x2a, 0x7f, 0x7f, 0xdb, 0x25,
            0x7f, 0x7f,
        ];
        let exp: &[u8] = &[
            0x00, 0x00, 0x7f, 0x7f, 0x00, 0x7f, 0x00, 0x81, 0x49, 0x49, 0x4a, 0x7f, 0xc1, 0x3f,
            0x5a, 0x7f,
        ];
        decode_filter_oct(&mut buf, 4, 4);
        assert_eq!(buf, exp);
    }

    #[test]
    fn filter_quat_matches_reference() {
        let mut buf: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x07, 0x00, 0x00, 0x00, 0x00, 0xff, 0x07,
            0xfc, 0x07, 0xa7, 0x05, 0xa7, 0x05, 0xa7, 0x05, 0xfc, 0x07,
        ];
        let exp: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x7f, 0x82, 0x5a, 0x00, 0x00, 0x00, 0x00,
            0x82, 0x5a, 0x0f, 0x40, 0xfa, 0x3f, 0xfa, 0x3f, 0xfa, 0x3f,
        ];
        decode_filter_quat(&mut buf, 3, 8);
        assert_eq!(buf, exp);
    }

    #[test]
    fn glb_meshopt_roundtrip_plumbing() {
        // A real meshopt-encoded ATTRIBUTES stream (6 verts × 16 bytes).
        let venc: &[u8] = &[
            0xa1, 0xff, 0xff, 0xff, 0xff, 0x00, 0x98, 0x98, 0x98, 0xd8, 0x58, 0x00, 0xa8, 0xa8,
            0xa8, 0x68, 0xe8, 0x00, 0x98, 0xb8, 0x98, 0x78, 0xd8, 0x00, 0x98, 0x98, 0x98, 0x98,
            0xd8, 0x00, 0xa8, 0xa8, 0xa8, 0xa8, 0x68, 0x00, 0x98, 0xb8, 0x98, 0xb8, 0x58, 0x00,
            0xa8, 0x88, 0xa8, 0xc8, 0x68, 0x00, 0xa8, 0xa8, 0xa8, 0x68, 0xe8, 0x00, 0x98, 0xb8,
            0x98, 0x78, 0xd8, 0x00, 0xa8, 0x88, 0xa8, 0x88, 0xe8, 0x00, 0x98, 0x98, 0x98, 0x98,
            0xd8, 0x00, 0x98, 0xb8, 0x98, 0xb8, 0x58, 0x00, 0xa8, 0x88, 0xa8, 0xc8, 0x68, 0x00,
            0x98, 0x98, 0x98, 0xd8, 0x58, 0x00, 0xa8, 0xa8, 0xa8, 0x68, 0xe8, 0x00, 0xa8, 0x88,
            0xa8, 0x88, 0xe8, 0x00, 0x00, 0x00, 0x00, 0x0d, 0x32, 0x57, 0x7c, 0xa0, 0xc7, 0xea,
            0x11, 0x37, 0x58, 0x7d, 0xa6, 0xca, 0xed, 0x10, 0x3b, 0x00, 0x00, 0x00, 0x00,
        ];
        let vexp: &[u8] = &[
            0x0d, 0x32, 0x57, 0x7c, 0xa0, 0xc7, 0xea, 0x11, 0x37, 0x58, 0x7d, 0xa6, 0xca, 0xed,
            0x10, 0x3b, 0x59, 0x86, 0xa3, 0xc8, 0xf4, 0x13, 0x3e, 0x65, 0x83, 0xac, 0xc9, 0xf2,
            0x1e, 0x39, 0x64, 0x8f, 0xa5, 0xda, 0xff, 0x14, 0x48, 0x6f, 0x82, 0xb9, 0xdf, 0xf0,
            0x15, 0x4e, 0x62, 0x85, 0xb8, 0xd3, 0xf1, 0x2e, 0x4b, 0x60, 0x9c, 0xbb, 0xd6, 0x0d,
            0x2b, 0x44, 0x61, 0x9a, 0xb6, 0xd1, 0x0c, 0x27, 0x5d, 0x62, 0x87, 0xac, 0xf0, 0x17,
            0x3a, 0x41, 0x67, 0x88, 0xad, 0xf6, 0x1a, 0x3d, 0x40, 0x6b, 0x89, 0xd6, 0xf3, 0x18,
            0x24, 0x43, 0x6e, 0xb5, 0xd3, 0xfc, 0x19, 0x22, 0x4e, 0x69, 0xb4, 0xdf,
        ];

        // Build a minimal GLB whose only buffer (the BIN) holds `venc`, with a
        // single meshopt-compressed ATTRIBUTES buffer view.
        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "extensionsUsed": ["EXT_meshopt_compression"],
            "extensionsRequired": ["EXT_meshopt_compression"],
            "buffers": [{"byteLength": venc.len()}],
            "bufferViews": [{
                "buffer": 0, "byteOffset": 0, "byteLength": vexp.len(), "byteStride": 16,
                "extensions": {"EXT_meshopt_compression": {
                    "buffer": 0, "byteOffset": 0, "byteLength": venc.len(),
                    "byteStride": 16, "count": 6, "mode": "ATTRIBUTES"
                }}
            }]
        });
        let mut json_bytes = serde_json::to_vec(&json).unwrap();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }
        let mut bin = venc.to_vec();
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
        let mut glb = Vec::new();
        glb.extend_from_slice(&GLB_MAGIC.to_le_bytes());
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(&GLB_CHUNK_JSON.to_le_bytes());
        glb.extend_from_slice(&json_bytes);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&GLB_CHUNK_BIN.to_le_bytes());
        glb.extend_from_slice(&bin);

        let out = decode_meshopt_glb(&glb).unwrap().expect("should decode");

        // Parse the output GLB and confirm bufferView 0 now points at decoded data.
        assert_eq!(rd_u32(&out, 0), Some(GLB_MAGIC));
        let jlen = rd_u32(&out, 12).unwrap() as usize;
        let oj: serde_json::Value = serde_json::from_slice(&out[20..20 + jlen]).unwrap();
        // extension stripped
        assert!(oj.get("extensionsRequired").is_none());
        assert!(
            oj["bufferViews"][0]
                .pointer("/extensions/EXT_meshopt_compression")
                .is_none()
        );
        let off = oj["bufferViews"][0]["byteOffset"].as_u64().unwrap() as usize;
        let len = oj["bufferViews"][0]["byteLength"].as_u64().unwrap() as usize;
        assert_eq!(len, vexp.len());
        // locate BIN chunk
        let bin_hdr = 20 + jlen;
        let blen = rd_u32(&out, bin_hdr).unwrap() as usize;
        let bin_start = bin_hdr + 8;
        let obin = &out[bin_start..bin_start + blen];
        assert_eq!(&obin[off..off + len], vexp);
    }

    #[test]
    fn filter_exp_matches_reference() {
        let mut buf: Vec<u8> = vec![
            0x00, 0x20, 0x00, 0xf3, 0x00, 0x38, 0x00, 0xf4, 0x00, 0xdc, 0xff, 0xf4, 0x00, 0x32,
            0x00, 0xf9,
        ];
        // expected = the four f32 values 1.0, 3.5, -2.25, 100.0
        let exp: &[u8] = &[
            0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x60, 0x40, 0x00, 0x00, 0x10, 0xc0, 0x00, 0x00,
            0xc8, 0x42,
        ];
        decode_filter_exp(&mut buf, 4, 4);
        assert_eq!(buf, exp);
    }
}
