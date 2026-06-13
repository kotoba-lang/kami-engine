//! KAMI columnar frame decoder — the Rust side of the clj↔Rust render-IR
//! contract (kami-engine-sdk-clj `kami.ipc/pack`). Pure, **no GPU deps**, so it
//! unit-tests headlessly. The browser host (`host.rs`, behind the `host` feature)
//! feeds the decoded camera + instance matrices to `kami-render`.
//!
//! Buffer layout (little-endian; see `kami.ipc` docstring):
//!
//! ```text
//! [Frame header        16B] magic 'KAMI' u32 | version u16 | ncols u16 |
//!                           frame_n u32 | pad u32
//! [Column header  16B × n]  dtype u8 | stride u8 | pad u16 | len u32 |
//!                           offset u32 | pad u32
//! [payload, 16B-aligned]    raw element bytes per column at its `offset`
//! ```
//!
//! Column 0 is always the camera (2 × mat4 = view, proj). Columns 1..n are the
//! per-draw instance model-matrix arrays, in the same order as the draw-table
//! that travels alongside as JSON meta.

/// ASCII 'KAMI' read as a little-endian u32 (bytes 4B 41 4D 49).
pub const MAGIC: u32 = 0x494D_414B;
pub const VERSION: u16 = 1;

pub const DTYPE_F32: u8 = 0;
pub const DTYPE_U32: u8 = 2;
pub const DTYPE_MAT4: u8 = 6;

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    TooShort,
    BadMagic(u32),
    BadVersion(u16),
    /// A column header carries a Dtype tag this decoder does not know. We refuse
    /// the frame rather than silently dropping the column (the clj packer rejects
    /// unknown dtypes symmetrically), so a producer/consumer version skew is loud.
    UnknownDtype { index: usize, dtype: u8 },
    ColumnOutOfBounds { index: usize, offset: usize, end: usize, buf: usize },
}

/// Element size in bytes for a Dtype tag (mirrors `kami-core::ipc::Dtype`).
pub fn element_size(dtype: u8) -> usize {
    match dtype {
        0 => 4,  // f32
        1 => 2,  // f16
        2 => 4,  // u32
        3 => 2,  // u16
        4 => 1,  // u8
        5 => 2,  // i16
        6 => 64, // mat4
        7 => 8,  // quat
        _ => 0,
    }
}

#[inline]
fn rd_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
#[inline]
fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
#[inline]
fn rd_f32(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// A borrowed view over one column's payload.
#[derive(Debug)]
pub struct ColumnView<'a> {
    pub dtype: u8,
    pub stride: u8,
    pub len: u32,
    pub data: &'a [u8],
}

impl<'a> ColumnView<'a> {
    /// Iterate this column as column-major `[f32; 16]` matrices (dtype must be
    /// mat4). `len` × `stride` matrices total.
    pub fn mat4s(&self) -> Vec<[f32; 16]> {
        debug_assert_eq!(self.dtype, DTYPE_MAT4);
        let count = self.len as usize * self.stride.max(1) as usize;
        (0..count)
            .map(|m| {
                let base = m * 64;
                let mut out = [0.0f32; 16];
                for (i, slot) in out.iter_mut().enumerate() {
                    *slot = rd_f32(self.data, base + i * 4);
                }
                out
            })
            .collect()
    }
}

/// A decoded frame: the frame number + every column in order.
#[derive(Debug)]
pub struct FrameView<'a> {
    pub frame_n: u32,
    pub columns: Vec<ColumnView<'a>>,
}

impl<'a> FrameView<'a> {
    /// The camera column (column 0): its two mat4s are `[view, proj]`.
    pub fn camera(&self) -> Option<([f32; 16], [f32; 16])> {
        let c = self.columns.first()?;
        let ms = c.mat4s();
        Some((*ms.first()?, *ms.get(1)?))
    }

    /// The per-draw instance columns (columns 1..n), one entry per draw.
    pub fn draw_instances(&self) -> &[ColumnView<'a>] {
        if self.columns.is_empty() {
            &[]
        } else {
            &self.columns[1..]
        }
    }
}

/// Decode a KAMI columnar buffer. Validates magic/version and bounds-checks every
/// column payload against the buffer length. Zero-copy: `ColumnView`s borrow `buf`.
pub fn decode(buf: &[u8]) -> Result<FrameView<'_>, DecodeError> {
    if buf.len() < 16 {
        return Err(DecodeError::TooShort);
    }
    let magic = rd_u32(buf, 0);
    if magic != MAGIC {
        return Err(DecodeError::BadMagic(magic));
    }
    let version = rd_u16(buf, 4);
    if version != VERSION {
        return Err(DecodeError::BadVersion(version));
    }
    let ncols = rd_u16(buf, 6) as usize;
    let frame_n = rd_u32(buf, 8);

    let mut columns = Vec::with_capacity(ncols);
    for i in 0..ncols {
        let h = 16 + i * 16;
        if h + 16 > buf.len() {
            return Err(DecodeError::TooShort);
        }
        let dtype = buf[h];
        let stride = buf[h + 1];
        let len = rd_u32(buf, h + 4);
        let offset = rd_u32(buf, h + 8) as usize;
        let esize = element_size(dtype);
        if esize == 0 {
            return Err(DecodeError::UnknownDtype { index: i, dtype });
        }
        let payload = esize * len as usize * stride.max(1) as usize;
        let end = offset + payload;
        if end > buf.len() {
            return Err(DecodeError::ColumnOutOfBounds {
                index: i,
                offset,
                end,
                buf: buf.len(),
            });
        }
        columns.push(ColumnView {
            dtype,
            stride,
            len,
            data: &buf[offset..end],
        });
    }
    Ok(FrameView { frame_n, columns })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact bytes emitted by `kami.ipc/pack` for the deterministic fixture
    /// scene (kami-engine-sdk-clj `dev/gen_fixture.clj`): a camera at z=+5 and two
    /// trees at x=±2. This is the cross-language contract anchor.
    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/frame.bin");

    #[test]
    fn decodes_clj_emitted_fixture() {
        let f = decode(FIXTURE).expect("fixture decodes");

        // header
        assert_eq!(f.frame_n, 42, "frame_n round-trips from clj");
        assert_eq!(f.columns.len(), 2, "camera column + 1 instanced draw column");

        // camera column = 2 mat4 (view, proj)
        let cam = f.columns.first().unwrap();
        assert_eq!(cam.dtype, DTYPE_MAT4);
        assert_eq!(cam.len, 2);
        let (view, proj) = f.camera().unwrap();
        // camera world translation is +5 on z → view (its inverse) is -5 on z.
        assert_eq!(view[14], -5.0, "view matrix z-translation");
        // perspective proj is right-handed wgpu (m[11] == -1, m[10] < 0).
        assert_eq!(proj[11], -1.0, "perspective w-row");
        assert!(proj[10] < 0.0, "perspective z-scale negative (RH, depth 0..1)");

        // instance column = 2 mat4 (two trees), x-translations are ±2 (order-free).
        let inst = &f.draw_instances()[0];
        assert_eq!(inst.dtype, DTYPE_MAT4);
        assert_eq!(inst.len, 2);
        let mats = inst.mat4s();
        let mut xs: Vec<f32> = mats.iter().map(|m| m[12]).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(xs, vec![-2.0, 2.0], "two tree instances at x = ±2");
        // each instance is an unrotated unit-scale model → m[0]==1, m[15]==1.
        for m in &mats {
            assert_eq!(m[0], 1.0);
            assert_eq!(m[15], 1.0);
        }
    }

    #[test]
    fn rejects_bad_magic() {
        let mut b = FIXTURE.to_vec();
        b[0] = 0;
        assert!(matches!(decode(&b), Err(DecodeError::BadMagic(_))));
    }

    #[test]
    fn rejects_unknown_dtype() {
        // corrupt column 0's dtype byte (header at offset 16) to an unknown tag.
        let mut b = FIXTURE.to_vec();
        b[16] = 9;
        assert!(matches!(
            decode(&b),
            Err(DecodeError::UnknownDtype { index: 0, dtype: 9 })
        ));
    }

    #[test]
    fn rejects_truncated() {
        assert!(matches!(decode(&FIXTURE[..8]), Err(DecodeError::TooShort)));
    }

    #[test]
    fn every_payload_offset_is_16_byte_aligned() {
        // mirror of the clj-side invariant: columns DMA into GPU buffers without
        // realignment, so each payload offset must be 16-aligned.
        let f = decode(FIXTURE).unwrap();
        // recompute offsets from the header to assert alignment
        for i in 0..f.columns.len() {
            let off = rd_u32(FIXTURE, 16 + i * 16 + 8) as usize;
            assert_eq!(off % 16, 0, "column {i} payload offset 16-aligned");
        }
    }
}
