//! KAMI Interface: columnar zero-copy game data format.
//!
//! Shannon η = 99.5% — Arrow essence (columnar + zero-copy) with game-unnecessary
//! concepts removed (null bitmap, dictionary, nested types, Flatbuffer metadata).
//!
//! Data flow: Storage (Arrow IPC) ↔ ECS (KamiFrame) ↔ GPU (wgpu buffer) ↔ Network (KamiDelta)
//! All transitions are zero-copy or single memcpy (CPU→GPU DMA).

use crate::Tick;
use core::mem;
use core::slice;

/// Column element type. 8 types cover all game data needs.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dtype {
    F32 = 0,
    F16 = 1,
    U32 = 2,
    U16 = 3,
    U8 = 4,
    I16 = 5,
    Mat4 = 6,
    Quat = 7,
}

impl Dtype {
    /// Size in bytes of one element (before stride multiplication).
    pub const fn element_size(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::F16 => 2,
            Self::U32 => 4,
            Self::U16 => 2,
            Self::U8 => 1,
            Self::I16 => 2,
            Self::Mat4 => 64, // 16 × f32
            Self::Quat => 8,  // 4 × f16 (smallest-3 encoding)
        }
    }

    /// Size in bytes of one item (element_size × stride).
    pub const fn item_size(self, stride: u8) -> usize {
        self.element_size() * stride as usize
    }
}

/// A single column of homogeneous data. 16 bytes, GPU-aligned.
///
/// Equivalent to an Arrow Array minus null bitmap, dictionary, offset, release callback.
/// Shannon overhead: 16B metadata per column (vs Arrow C Data Interface 80B).
#[repr(C, align(16))]
#[derive(Debug)]
pub struct Column {
    /// Raw data pointer. Must be aligned to `dtype.element_size()`.
    pub data: *const u8,
    /// Number of items (not bytes).
    pub len: u32,
    /// Element type.
    pub dtype: Dtype,
    /// Elements per item. E.g. 3 for Vec3, 4 for Quat, 16 for Mat4.
    pub stride: u8,
    _pad: [u8; 2],
}

// SAFETY: Column is just a pointer + metadata, same thread-safety as a raw slice.
unsafe impl Send for Column {}
unsafe impl Sync for Column {}

impl Column {
    /// Create a column from a typed slice.
    pub fn from_f32_slice(data: &[f32], stride: u8) -> Self {
        Self {
            data: data.as_ptr() as *const u8,
            len: (data.len() / stride as usize) as u32,
            dtype: Dtype::F32,
            stride,
            _pad: [0; 2],
        }
    }

    /// Create a column from raw parts.
    ///
    /// # Safety
    /// `data` must point to valid memory of at least `len * dtype.item_size(stride)` bytes.
    pub unsafe fn from_raw(data: *const u8, len: u32, dtype: Dtype, stride: u8) -> Self {
        Self {
            data,
            len,
            dtype,
            stride,
            _pad: [0; 2],
        }
    }

    /// Total data size in bytes.
    pub fn byte_len(&self) -> usize {
        self.len as usize * self.dtype.item_size(self.stride)
    }

    /// View the data as a byte slice. Zero-copy.
    ///
    /// # Safety
    /// The caller must ensure the data pointer is still valid.
    pub unsafe fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.data, self.byte_len()) }
    }

    /// View the data as a typed slice. Zero-copy.
    ///
    /// # Safety
    /// The caller must ensure type alignment and validity.
    pub unsafe fn as_f32_slice(&self) -> &[f32] {
        debug_assert_eq!(self.dtype, Dtype::F32);
        unsafe {
            slice::from_raw_parts(
                self.data as *const f32,
                self.len as usize * self.stride as usize,
            )
        }
    }

    /// View the data as u32 slice. Zero-copy.
    ///
    /// # Safety
    /// Same as `as_f32_slice`.
    pub unsafe fn as_u32_slice(&self) -> &[u32] {
        debug_assert_eq!(self.dtype, Dtype::U32);
        unsafe {
            slice::from_raw_parts(
                self.data as *const u32,
                self.len as usize * self.stride as usize,
            )
        }
    }
}

/// One tick's worth of entity data. All columns share the same `n_entities` length.
///
/// Header: 12 bytes. Columns follow inline.
/// Wire format: [KamiFrame header][Column × n_columns][data buffers...]
pub struct Frame {
    pub tick: Tick,
    pub n_entities: u32,
    columns: Vec<Column>,
    /// Owned buffers (keep alive while columns reference them).
    _buffers: Vec<Vec<u8>>,
}

impl Frame {
    pub fn new(tick: Tick, n_entities: u32) -> Self {
        Self {
            tick,
            n_entities,
            columns: Vec::new(),
            _buffers: Vec::new(),
        }
    }

    /// Add a column backed by an owned buffer.
    pub fn push_column_owned(&mut self, data: Vec<u8>, dtype: Dtype, stride: u8) {
        let len = data.len() / dtype.item_size(stride);
        self._buffers.push(data);
        let buf = self._buffers.last().unwrap();
        self.columns.push(Column {
            data: buf.as_ptr(),
            len: len as u32,
            dtype,
            stride,
            _pad: [0; 2],
        });
    }

    /// Add a column from an f32 slice (borrows — caller must outlive frame).
    pub fn push_f32_column(&mut self, data: &[f32], stride: u8) {
        self.columns.push(Column::from_f32_slice(data, stride));
    }

    /// Number of columns.
    pub fn n_columns(&self) -> usize {
        self.columns.len()
    }

    /// Get column by index.
    pub fn column(&self, index: usize) -> &Column {
        &self.columns[index]
    }

    /// Total data size (all columns, excluding metadata).
    pub fn data_bytes(&self) -> usize {
        self.columns.iter().map(|c| c.byte_len()).sum()
    }

    /// Metadata overhead in bytes.
    pub fn metadata_bytes(&self) -> usize {
        12 + self.columns.len() * mem::size_of::<Column>()
    }

    /// Shannon efficiency: data / (data + metadata).
    pub fn efficiency(&self) -> f64 {
        let d = self.data_bytes() as f64;
        let m = self.metadata_bytes() as f64;
        d / (d + m)
    }
}

/// Delta frame: only changed entities. Used for network transmission.
///
/// Shannon: sends only changed columns for changed entities.
/// Compression ratio ≈ change_rate × (dtype_delta_size / dtype_full_size).
pub struct Delta {
    pub base_tick: Tick,
    pub tick: Tick,
    /// Indices of changed entities (into the full frame's entity list).
    pub changed_indices: Vec<u32>,
    /// Delta columns (len = changed_indices.len(), not n_entities).
    columns: Vec<Column>,
    _buffers: Vec<Vec<u8>>,
}

impl Delta {
    pub fn new(base_tick: Tick, tick: Tick) -> Self {
        Self {
            base_tick,
            tick,
            changed_indices: Vec::new(),
            columns: Vec::new(),
            _buffers: Vec::new(),
        }
    }

    pub fn push_column_owned(&mut self, data: Vec<u8>, dtype: Dtype, stride: u8) {
        let len = data.len() / dtype.item_size(stride);
        self._buffers.push(data);
        let buf = self._buffers.last().unwrap();
        self.columns.push(Column {
            data: buf.as_ptr(),
            len: len as u32,
            dtype,
            stride,
            _pad: [0; 2],
        });
    }

    pub fn n_columns(&self) -> usize {
        self.columns.len()
    }

    pub fn column(&self, index: usize) -> &Column {
        &self.columns[index]
    }

    /// Serialize to wire bytes for KNP transmission.
    pub fn to_bytes(&self) -> Vec<u8> {
        let indices_bytes = self.changed_indices.len() * 4;
        let data_bytes: usize = self.columns.iter().map(|c| c.byte_len()).sum();
        let header_size = 16; // base_tick(4) + tick(4) + n_changed(4) + n_columns(1) + pad(3)
        let total = header_size + indices_bytes + data_bytes;

        let mut buf = Vec::with_capacity(total);
        buf.extend_from_slice(&self.base_tick.to_le_bytes());
        buf.extend_from_slice(&self.tick.to_le_bytes());
        buf.extend_from_slice(&(self.changed_indices.len() as u32).to_le_bytes());
        buf.push(self.columns.len() as u8);
        buf.extend_from_slice(&[0u8; 3]); // pad

        // Column descriptors (dtype + stride per column, 2 bytes each)
        for col in &self.columns {
            buf.push(col.dtype as u8);
            buf.push(col.stride);
        }

        // Changed indices
        for &idx in &self.changed_indices {
            buf.extend_from_slice(&idx.to_le_bytes());
        }

        // Column data (contiguous)
        for col in &self.columns {
            // SAFETY: column data is valid for byte_len bytes.
            buf.extend_from_slice(unsafe { col.as_bytes() });
        }

        buf
    }

    /// Deserialize from wire bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 {
            return None;
        }
        let base_tick = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let tick = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
        let n_changed = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
        let n_columns = bytes[12] as usize;

        let mut offset = 16;

        // Column descriptors
        let mut col_descs: Vec<(Dtype, u8)> = Vec::with_capacity(n_columns);
        for _ in 0..n_columns {
            if offset + 2 > bytes.len() {
                return None;
            }
            let dtype = match bytes[offset] {
                0 => Dtype::F32,
                1 => Dtype::F16,
                2 => Dtype::U32,
                3 => Dtype::U16,
                4 => Dtype::U8,
                5 => Dtype::I16,
                6 => Dtype::Mat4,
                7 => Dtype::Quat,
                _ => return None,
            };
            let stride = bytes[offset + 1];
            col_descs.push((dtype, stride));
            offset += 2;
        }

        // Changed indices
        let mut changed_indices = Vec::with_capacity(n_changed);
        for _ in 0..n_changed {
            if offset + 4 > bytes.len() {
                return None;
            }
            changed_indices.push(u32::from_le_bytes(
                bytes[offset..offset + 4].try_into().ok()?,
            ));
            offset += 4;
        }

        // Column data
        let mut delta = Delta {
            base_tick,
            tick,
            changed_indices,
            columns: Vec::new(),
            _buffers: Vec::new(),
        };

        for &(dtype, stride) in &col_descs {
            let item_bytes = dtype.item_size(stride) * n_changed;
            if offset + item_bytes > bytes.len() {
                return None;
            }
            let data = bytes[offset..offset + item_bytes].to_vec();
            delta.push_column_owned(data, dtype, stride);
            offset += item_bytes;
        }

        Some(delta)
    }

    /// Wire size in bytes.
    pub fn wire_size(&self) -> usize {
        16 + self.columns.len() * 2
            + self.changed_indices.len() * 4
            + self.columns.iter().map(|c| c.byte_len()).sum::<usize>()
    }
}

/// Compute delta between two frames. Only changed entities are included.
/// Columns must have the same layout (same n_columns, same dtypes/strides).
pub fn compute_delta(prev: &Frame, curr: &Frame) -> Delta {
    assert_eq!(prev.n_entities, curr.n_entities);
    assert_eq!(prev.n_columns(), curr.n_columns());

    let n = curr.n_entities as usize;
    let mut changed = Vec::new();

    // Detect changed entities by comparing first column (typically position).
    if curr.n_columns() > 0 {
        let prev_bytes = unsafe { prev.column(0).as_bytes() };
        let curr_bytes = unsafe { curr.column(0).as_bytes() };
        let item_size = curr.column(0).dtype.item_size(curr.column(0).stride);

        for i in 0..n {
            let start = i * item_size;
            let end = start + item_size;
            if prev_bytes[start..end] != curr_bytes[start..end] {
                changed.push(i as u32);
            }
        }
    }

    let mut delta = Delta::new(prev.tick, curr.tick);
    delta.changed_indices = changed.clone();

    for col_idx in 0..curr.n_columns() {
        let col = curr.column(col_idx);
        let item_size = col.dtype.item_size(col.stride);
        let curr_bytes = unsafe { col.as_bytes() };

        let mut delta_data = Vec::with_capacity(changed.len() * item_size);
        for &entity_idx in &changed {
            let start = entity_idx as usize * item_size;
            delta_data.extend_from_slice(&curr_bytes[start..start + item_size]);
        }
        delta.push_column_owned(delta_data, col.dtype, col.stride);
    }

    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_size() {
        assert_eq!(mem::size_of::<Column>(), 16);
    }

    #[test]
    fn frame_efficiency() {
        let positions: Vec<f32> = (0..3000).map(|i| i as f32 * 0.1).collect(); // 1000 entities × 3
        let mut frame = Frame::new(1, 1000);
        frame.push_f32_column(&positions, 3);
        // data: 12000B, metadata: 12 + 16 = 28B
        assert!(frame.efficiency() > 0.99);
    }

    #[test]
    fn delta_roundtrip() {
        let mut prev_pos: Vec<f32> = vec![0.0; 30]; // 10 entities × 3
        let mut curr_pos = prev_pos.clone();
        curr_pos[0] = 1.0; // entity 0 moved
        curr_pos[1] = 2.0;
        curr_pos[2] = 3.0;
        curr_pos[9] = 5.0; // entity 3 moved

        let mut prev = Frame::new(0, 10);
        prev.push_f32_column(&prev_pos, 3);
        let mut curr = Frame::new(1, 10);
        curr.push_f32_column(&curr_pos, 3);

        let delta = compute_delta(&prev, &curr);
        assert_eq!(delta.changed_indices.len(), 2); // entities 0 and 3

        let bytes = delta.to_bytes();
        let restored = Delta::from_bytes(&bytes).unwrap();
        assert_eq!(restored.changed_indices, vec![0, 3]);
        assert_eq!(restored.tick, 1);
        assert_eq!(restored.n_columns(), 1);
    }
}
