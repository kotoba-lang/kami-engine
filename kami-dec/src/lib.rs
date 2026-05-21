//! kami-dec — Discrete Exterior Calculus on the voxel cubical complex.
//!
//! This is the **v3 physics prototype**. Instead of N hand-coded rules
//! (v2 `kami-pipelines::voxel::VoxelRule`), physical behaviour is
//! expressed as `∂_t φ = L(φ)` where `L` is a composition of the three
//! DEC primitives:
//!
//!   - `d` — exterior derivative (`d_k: Λ^k → Λ^{k+1}`). On a voxel
//!     lattice with cubical cells: `d_0` is discrete gradient along
//!     edges, `d_1` is curl through faces, `d_2` is divergence out of
//!     volumes.
//!   - `*` — Hodge star (`*_k: Λ^k → Λ^{n-k}`). On a flat 3D grid with
//!     unit spacing the Hodge star is diagonal (identity up to a metric
//!     factor) so we can defer its full implementation.
//!   - `Δ = *d*d + d*d*` — Laplacian. On 0-forms: `Δ φ = d* d φ` which
//!     reduces to the 7-point stencil `(Σ neighbours) − 6 φ_i` on a
//!     6-connected voxel graph.
//!
//! # What ships here
//!
//! - [`ScalarField`] — 0-form on voxel cell centres. Sparse storage
//!   keyed by chunk coord, so unused chunks cost nothing.
//! - [`ScalarField::diffuse`] — explicit Euler step of the heat
//!   equation `∂_t φ = α Δ φ` using only loaded neighbour chunks.
//!   Stable when `α · dt ≤ 1/6`.
//! - [`ScalarField::emit_from`] — source term driven by voxel material.
//!
//! # Why not just scalar fields?
//!
//! The `ScalarField` alone already covers heat / moisture / smoke
//! density. Richer dynamics (vorticity, wave propagation, divergence-
//! free flows) need 1-forms on edges and 2-forms on faces. Those
//! modules (`EdgeField`, `FaceField`, `d_1`, `d_2`) are stubbed below
//! and will be filled in when the prototype proves out.

use std::collections::HashMap;

/// Voxel chunk size in cells per axis. Must match `kami-pipelines::CHUNK_SIZE`.
pub const CHUNK_SIZE: usize = 16;
/// Voxels per chunk.
pub const CHUNK_CELLS: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Chunk coordinate in the lattice (1 unit = `CHUNK_SIZE` world meters).
pub type ChunkCoord = (i32, i32, i32);

/// 0-form: one scalar per voxel cell centre, stored per chunk.
///
/// Sparse — chunks allocate on first write. Reading an absent chunk
/// returns 0.0. Numerically stable under explicit Euler with the
/// provided `diffuse()` step when `α * dt ≤ 1/6`.
pub struct ScalarField {
    pub chunks: HashMap<ChunkCoord, Box<[f32; CHUNK_CELLS]>>,
}

impl Default for ScalarField {
    fn default() -> Self {
        Self::new()
    }
}

impl ScalarField {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
        }
    }

    /// Read at world voxel coordinate. Absent chunk = 0.0.
    pub fn get(&self, x: i32, y: i32, z: i32) -> f32 {
        let cc = Self::chunk_coord(x, y, z);
        let li = Self::local_index(x, y, z);
        self.chunks.get(&cc).map(|c| c[li]).unwrap_or(0.0)
    }

    /// Write at world voxel coordinate, creating the chunk if needed.
    pub fn set(&mut self, x: i32, y: i32, z: i32, val: f32) {
        let cc = Self::chunk_coord(x, y, z);
        let li = Self::local_index(x, y, z);
        let chunk = self
            .chunks
            .entry(cc)
            .or_insert_with(|| Box::new([0.0; CHUNK_CELLS]));
        chunk[li] = val;
    }

    /// In-place add. Convenience for source terms.
    pub fn add(&mut self, x: i32, y: i32, z: i32, delta: f32) {
        let cc = Self::chunk_coord(x, y, z);
        let li = Self::local_index(x, y, z);
        let chunk = self
            .chunks
            .entry(cc)
            .or_insert_with(|| Box::new([0.0; CHUNK_CELLS]));
        chunk[li] += delta;
    }

    /// Diffusion step (heat equation): `φ_new = φ + α · dt · Δφ`.
    ///
    /// Discrete Laplacian on a 6-connected voxel graph:
    ///   Δφ_i = (Σ_{j ∈ N(i)} φ_j) − 6 φ_i
    ///
    /// For stability we require `α · dt ≤ 1/6`. Higher values ring.
    ///
    /// Also optionally applies a linear decay `φ *= 1 - k·dt` to model
    /// ambient dissipation (smoke fading, heat radiation into the
    /// environment). Pass `decay = 0.0` for pure diffusion.
    pub fn diffuse(&mut self, alpha: f32, dt: f32, decay: f32) {
        let coeff = alpha * dt;
        debug_assert!(coeff <= 1.0 / 6.0 + 1e-6, "diffuse unstable: α·dt > 1/6");

        // Snapshot the old values so the update is simultaneous (we
        // can't mutate and read the same buffer for Laplacian).
        let old = ScalarField {
            chunks: self
                .chunks
                .iter()
                .map(|(c, v)| (*c, v.clone()))
                .collect(),
        };
        let decay_coeff = (1.0 - decay * dt).max(0.0);

        // Also activate 1-neighbour-wider chunks so diffusion can leak
        // into fresh voxels at chunk boundaries.
        let mut active: Vec<ChunkCoord> = Vec::new();
        for &cc in old.chunks.keys() {
            active.push(cc);
            for (dx, dy, dz) in [
                (-1, 0, 0), (1, 0, 0),
                (0, -1, 0), (0, 1, 0),
                (0, 0, -1), (0, 0, 1),
            ] {
                let ncc = (cc.0 + dx, cc.1 + dy, cc.2 + dz);
                if !active.contains(&ncc) {
                    active.push(ncc);
                }
            }
        }

        for cc in active {
            // Pull self + 6 neighbour chunks (for boundary stencils).
            let base_x = cc.0 * CHUNK_SIZE as i32;
            let base_y = cc.1 * CHUNK_SIZE as i32;
            let base_z = cc.2 * CHUNK_SIZE as i32;

            // Early-out: if self + all 6 neighbours are empty, skip.
            let any_nonzero = old.chunks.contains_key(&cc)
                || old.chunks.contains_key(&(cc.0 - 1, cc.1, cc.2))
                || old.chunks.contains_key(&(cc.0 + 1, cc.1, cc.2))
                || old.chunks.contains_key(&(cc.0, cc.1 - 1, cc.2))
                || old.chunks.contains_key(&(cc.0, cc.1 + 1, cc.2))
                || old.chunks.contains_key(&(cc.0, cc.1, cc.2 - 1))
                || old.chunks.contains_key(&(cc.0, cc.1, cc.2 + 1));
            if !any_nonzero {
                continue;
            }

            let dst = self
                .chunks
                .entry(cc)
                .or_insert_with(|| Box::new([0.0; CHUNK_CELLS]));

            for lz in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let wx = base_x + lx as i32;
                        let wy = base_y + ly as i32;
                        let wz = base_z + lz as i32;
                        let p = old.get(wx, wy, wz);
                        // 7-point Laplacian stencil.
                        let nsum = old.get(wx + 1, wy, wz)
                            + old.get(wx - 1, wy, wz)
                            + old.get(wx, wy + 1, wz)
                            + old.get(wx, wy - 1, wz)
                            + old.get(wx, wy, wz + 1)
                            + old.get(wx, wy, wz - 1);
                        let lap = nsum - 6.0 * p;
                        let next = (p + coeff * lap) * decay_coeff;
                        let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                        dst[i] = next;
                    }
                }
            }
        }

        // Optional: prune chunks that have gone to ~0 to reclaim memory.
        self.chunks.retain(|_, c| c.iter().any(|v| v.abs() > 1e-4));
    }

    /// Source term: emit a value per solid voxel whose material passes
    /// the provided predicate. `source(material)` returns `Some(emit_rate)`
    /// for emitters, `None` for non-emitters. Called per emitting voxel
    /// per invocation, so `diff = source(mat) * dt` should be passed.
    ///
    /// Generic over the voxel reader so this crate doesn't depend on
    /// `kami-pipelines::VoxelChunkAdapter` directly.
    pub fn emit_from<F>(
        &mut self,
        emitters_world_space: impl Iterator<Item = (i32, i32, i32, u8)>,
        source: F,
        dt: f32,
    ) where
        F: Fn(u8) -> Option<f32>,
    {
        for (x, y, z, mat) in emitters_world_space {
            if let Some(rate) = source(mat) {
                self.add(x, y, z, rate * dt);
            }
        }
    }

    /// Iterate every cell above `threshold` in magnitude. Yields
    /// `(world_x, world_y, world_z, value)`. Order is chunk-major then
    /// cell-major; stable within a frame but not across ticks (HashMap
    /// ordering). Used by `kami-pipelines::FieldVisAdapter` for
    /// visualisation and could also drive emit-to-particle effects.
    pub fn for_each_nonzero<F: FnMut(i32, i32, i32, f32)>(&self, threshold: f32, mut f: F) {
        for (&(ccx, ccy, ccz), cells) in self.chunks.iter() {
            let base_x = ccx * CHUNK_SIZE as i32;
            let base_y = ccy * CHUNK_SIZE as i32;
            let base_z = ccz * CHUNK_SIZE as i32;
            for lz in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let idx = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                        let v = cells[idx];
                        if v.abs() > threshold {
                            f(base_x + lx as i32, base_y + ly as i32, base_z + lz as i32, v);
                        }
                    }
                }
            }
        }
    }

    /// Memory footprint (bytes).
    pub fn memory_bytes(&self) -> usize {
        self.chunks.len() * CHUNK_CELLS * std::mem::size_of::<f32>()
    }

    /// Active chunk count.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Drop chunks outside an AABB of `chunk_radius` around `center`.
    /// Active-region clip: after a few ticks of diffusion-driven
    /// spread, chunks that drift out of the designer-authored zone
    /// of interest get dropped rather than iterated forever.
    pub fn prune_outside(&mut self, center: ChunkCoord, chunk_radius: i32) {
        let r = chunk_radius;
        self.chunks.retain(|&cc, _| {
            (cc.0 - center.0).abs() <= r
                && (cc.1 - center.1).abs() <= r
                && (cc.2 - center.2).abs() <= r
        });
    }
}

// ───────────────────────────────────────────────────────────────────
// EdgeField — 1-form on voxel edges
// ───────────────────────────────────────────────────────────────────
//
// Each voxel cell owns three outgoing edges (to +X, +Y, +Z neighbours).
// Storage: `[f32; 3]` per cell — 48 KB per 16³ chunk (3× ScalarField).
//
// Use cases:
//   - wind / flow velocity (each edge = velocity component along it)
//   - discrete gradient `d_0(ScalarField) -> EdgeField`
//   - divergence-free projection (d_2 ∘ d_0 = 0 exactly on the lattice)
//   - advection of scalar / edge fields
//
// Stored sparsely exactly like ScalarField.

pub struct EdgeField {
    /// Per cell: `[+X edge, +Y edge, +Z edge]`.
    pub chunks: HashMap<ChunkCoord, Box<[[f32; 3]; CHUNK_CELLS]>>,
}

impl Default for EdgeField {
    fn default() -> Self { Self::new() }
}

impl EdgeField {
    pub fn new() -> Self { Self { chunks: HashMap::new() } }

    /// Raw edge value along axis `a` (0=X, 1=Y, 2=Z) at cell (x,y,z).
    pub fn get(&self, x: i32, y: i32, z: i32, a: usize) -> f32 {
        let cc = ScalarField::chunk_coord(x, y, z);
        let li = ScalarField::local_index(x, y, z);
        self.chunks.get(&cc).map(|c| c[li][a]).unwrap_or(0.0)
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, a: usize, val: f32) {
        let cc = ScalarField::chunk_coord(x, y, z);
        let li = ScalarField::local_index(x, y, z);
        let chunk = self.chunks.entry(cc).or_insert_with(|| Box::new([[0.0; 3]; CHUNK_CELLS]));
        chunk[li][a] = val;
    }

    /// Sample the full vector at a cell centre.
    pub fn vec_at(&self, x: i32, y: i32, z: i32) -> glam::Vec3 {
        glam::Vec3::new(
            self.get(x, y, z, 0),
            self.get(x, y, z, 1),
            self.get(x, y, z, 2),
        )
    }

    /// Fill every cell in the loaded footprint with a uniform vector.
    /// Useful for a constant wind; prefer `d_0` for gradient-driven flows.
    pub fn fill_uniform(&mut self, min: (i32, i32, i32), max: (i32, i32, i32), wind: glam::Vec3) {
        for z in min.2..max.2 {
            for y in min.1..max.1 {
                for x in min.0..max.0 {
                    self.set(x, y, z, 0, wind.x);
                    self.set(x, y, z, 1, wind.y);
                    self.set(x, y, z, 2, wind.z);
                }
            }
        }
    }

    pub fn chunk_count(&self) -> usize { self.chunks.len() }
    pub fn memory_bytes(&self) -> usize {
        self.chunks.len() * CHUNK_CELLS * 3 * std::mem::size_of::<f32>()
    }

    /// Drop chunks outside an AABB of `chunk_radius` around `center`.
    /// Used as an active-region clip so projection / advection /
    /// smoothing don't iterate far-away empty chunks that were
    /// allocated by a previous wider flow.
    pub fn prune_outside(&mut self, center: ChunkCoord, chunk_radius: i32) {
        let r = chunk_radius;
        self.chunks.retain(|&cc, _| {
            (cc.0 - center.0).abs() <= r
                && (cc.1 - center.1).abs() <= r
                && (cc.2 - center.2).abs() <= r
        });
    }

    /// Multiply every edge value by `factor`. Used for per-frame linear
    /// damping when the wind field is persisted across ticks (Boussinesq
    /// self-driven flow).
    pub fn damp(&mut self, factor: f32) {
        for cells in self.chunks.values_mut() {
            for c in cells.iter_mut() {
                c[0] *= factor;
                c[1] *= factor;
                c[2] *= factor;
            }
        }
    }

    /// Populate every cell in the scalar field's active chunk footprint
    /// with a uniform wind vector. Used to seed a background breeze
    /// before adding gradient-driven or buoyancy-driven contributions.
    /// Clears any previous edge values in those chunks.
    pub fn populate_from_scalar_footprint(
        &mut self,
        field: &ScalarField,
        uniform_wind: glam::Vec3,
    ) {
        for &cc in field.chunks.keys() {
            let base_x = cc.0 * CHUNK_SIZE as i32;
            let base_y = cc.1 * CHUNK_SIZE as i32;
            let base_z = cc.2 * CHUNK_SIZE as i32;
            let entry = self
                .chunks
                .entry(cc)
                .or_insert_with(|| Box::new([[0.0; 3]; CHUNK_CELLS]));
            for lz in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                        entry[i][0] = uniform_wind.x;
                        entry[i][1] = uniform_wind.y;
                        entry[i][2] = uniform_wind.z;
                    }
                }
            }
            let _ = (base_x, base_y, base_z);
        }
    }

    /// Zero out edges whose endpoints touch a solid voxel.
    ///
    /// A `+X` edge at cell `(x,y,z)` connects to cell `(x+1,y,z)`; if
    /// either endpoint is solid we can't let flow cross it. Same for
    /// Y/Z. This is the simplest no-slip / no-penetration boundary
    /// condition — wind dies at walls without any special Poisson
    /// handling. Works because the Laplacian + `div` stencils read
    /// these zeroed edges and naturally produce near-zero pressure
    /// gradients into the solid.
    pub fn mask_solid<F: Fn(i32, i32, i32) -> bool>(&mut self, solid: F) {
        for (&cc, cells) in self.chunks.iter_mut() {
            let bx = cc.0 * CHUNK_SIZE as i32;
            let by = cc.1 * CHUNK_SIZE as i32;
            let bz = cc.2 * CHUNK_SIZE as i32;
            for lz in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let x = bx + lx as i32;
                        let y = by + ly as i32;
                        let z = bz + lz as i32;
                        let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                        let me = solid(x, y, z);
                        if me || solid(x + 1, y, z) { cells[i][0] = 0.0; }
                        if me || solid(x, y + 1, z) { cells[i][1] = 0.0; }
                        if me || solid(x, y, z + 1) { cells[i][2] = 0.0; }
                    }
                }
            }
        }
    }

    /// Thermal buoyancy: "hot air rises". For each cell where `field`
    /// exceeds `threshold`, add `scale · value` to the +Y edge.
    /// Emergent wind — no hand-coded rule; heat itself produces updraft
    /// via the same DEC vocabulary as diffusion / advection.
    pub fn add_buoyancy_from(&mut self, field: &ScalarField, scale: f32, threshold: f32) {
        field.for_each_nonzero(threshold, |x, y, z, v| {
            let current = self.get(x, y, z, 1);
            self.set(x, y, z, 1, current + scale * v);
        });
    }
}

// ScalarField now exposes its coord helpers so EdgeField can piggy-back.
impl ScalarField {
    #[inline]
    pub fn chunk_coord(x: i32, y: i32, z: i32) -> ChunkCoord {
        let cs = CHUNK_SIZE as i32;
        (x.div_euclid(cs), y.div_euclid(cs), z.div_euclid(cs))
    }

    #[inline]
    pub fn local_index(x: i32, y: i32, z: i32) -> usize {
        let cs = CHUNK_SIZE as i32;
        (x.rem_euclid(cs) as usize)
            + (y.rem_euclid(cs) as usize) * CHUNK_SIZE
            + (z.rem_euclid(cs) as usize) * CHUNK_SIZE * CHUNK_SIZE
    }

    /// Trilinear sample at fractional world position.
    /// Underpins semi-Lagrangian advection (back-trace sampling).
    pub fn sample_trilinear(&self, p: glam::Vec3) -> f32 {
        let x0 = p.x.floor() as i32;
        let y0 = p.y.floor() as i32;
        let z0 = p.z.floor() as i32;
        let tx = p.x - p.x.floor();
        let ty = p.y - p.y.floor();
        let tz = p.z - p.z.floor();
        let c000 = self.get(x0, y0, z0);
        let c100 = self.get(x0 + 1, y0, z0);
        let c010 = self.get(x0, y0 + 1, z0);
        let c110 = self.get(x0 + 1, y0 + 1, z0);
        let c001 = self.get(x0, y0, z0 + 1);
        let c101 = self.get(x0 + 1, y0, z0 + 1);
        let c011 = self.get(x0, y0 + 1, z0 + 1);
        let c111 = self.get(x0 + 1, y0 + 1, z0 + 1);
        let c00 = c000 * (1.0 - tx) + c100 * tx;
        let c10 = c010 * (1.0 - tx) + c110 * tx;
        let c01 = c001 * (1.0 - tx) + c101 * tx;
        let c11 = c011 * (1.0 - tx) + c111 * tx;
        let c0 = c00 * (1.0 - ty) + c10 * ty;
        let c1 = c01 * (1.0 - ty) + c11 * ty;
        c0 * (1.0 - tz) + c1 * tz
    }

    /// Semi-Lagrangian advection under a uniform wind vector.
    /// Unconditionally stable (no CFL restriction) — classic Jos Stam
    /// "Stable Fluids" trick. Some numerical diffusion is introduced by
    /// the trilinear sample; pair with `diffuse` to hide it, or use a
    /// MacCormack correction (future).
    pub fn advect_uniform(&mut self, wind: glam::Vec3, dt: f32) {
        let old = ScalarField {
            chunks: self.chunks.iter().map(|(c, v)| (*c, v.clone())).collect(),
        };
        let step = wind * dt;
        for (cc, _) in old.chunks.iter() {
            let base_x = cc.0 * CHUNK_SIZE as i32;
            let base_y = cc.1 * CHUNK_SIZE as i32;
            let base_z = cc.2 * CHUNK_SIZE as i32;
            let dst = self.chunks.entry(*cc).or_insert_with(|| Box::new([0.0; CHUNK_CELLS]));
            for lz in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                        let wp = glam::Vec3::new(
                            (base_x + lx as i32) as f32,
                            (base_y + ly as i32) as f32,
                            (base_z + lz as i32) as f32,
                        );
                        let back = wp - step;
                        dst[i] = old.sample_trilinear(back);
                    }
                }
            }
        }
        self.chunks.retain(|_, c| c.iter().any(|v| v.abs() > 1e-4));
    }

    /// Semi-Lagrangian advection under a spatially-varying wind field.
    pub fn advect_field(&mut self, wind: &EdgeField, dt: f32) {
        let old = ScalarField {
            chunks: self.chunks.iter().map(|(c, v)| (*c, v.clone())).collect(),
        };
        for (cc, _) in old.chunks.iter() {
            let base_x = cc.0 * CHUNK_SIZE as i32;
            let base_y = cc.1 * CHUNK_SIZE as i32;
            let base_z = cc.2 * CHUNK_SIZE as i32;
            let dst = self.chunks.entry(*cc).or_insert_with(|| Box::new([0.0; CHUNK_CELLS]));
            for lz in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                        let wx = base_x + lx as i32;
                        let wy = base_y + ly as i32;
                        let wz = base_z + lz as i32;
                        let w = wind.vec_at(wx, wy, wz);
                        let wp = glam::Vec3::new(wx as f32, wy as f32, wz as f32);
                        dst[i] = old.sample_trilinear(wp - w * dt);
                    }
                }
            }
        }
        self.chunks.retain(|_, c| c.iter().any(|v| v.abs() > 1e-4));
    }
}

/// Exterior derivative `d_0 : Λ⁰ → Λ¹` (discrete gradient).
///
/// On the voxel lattice with unit spacing, the forward-difference
/// gradient is exact for linear fields:
///   (dφ)_{i,j,k}^{x} = φ_{i+1,j,k} − φ_{i,j,k}
///   (dφ)_{i,j,k}^{y} = φ_{i,j+1,k} − φ_{i,j,k}
///   (dφ)_{i,j,k}^{z} = φ_{i,j,k+1} − φ_{i,j,k}
///
/// Composing with the codifferential gives Δ = −d⁎d, matching the
/// 7-point Laplacian in `diffuse`. `d_0 ∘ d_0 = 0` (a 1-form
/// gradient has zero curl, the Hodge-theoretic analogue of
/// `∇ × ∇φ = 0`).
pub fn d_0(field: &ScalarField) -> EdgeField {
    let mut out = EdgeField::new();
    for (&cc, _cells) in field.chunks.iter() {
        let base_x = cc.0 * CHUNK_SIZE as i32;
        let base_y = cc.1 * CHUNK_SIZE as i32;
        let base_z = cc.2 * CHUNK_SIZE as i32;
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let x = base_x + lx as i32;
                    let y = base_y + ly as i32;
                    let z = base_z + lz as i32;
                    let p = field.get(x, y, z);
                    out.set(x, y, z, 0, field.get(x + 1, y, z) - p);
                    out.set(x, y, z, 1, field.get(x, y + 1, z) - p);
                    out.set(x, y, z, 2, field.get(x, y, z + 1) - p);
                }
            }
        }
    }
    out
}

/// Codifferential applied to a 1-form → 0-form (discrete divergence).
/// `div(e)_{i,j,k} = (e_{i,j,k}^x − e_{i−1,j,k}^x)
///                 + (e_{i,j,k}^y − e_{i,j−1,k}^y)
///                 + (e_{i,j,k}^z − e_{i,j,k−1}^z)`
pub fn div(e: &EdgeField) -> ScalarField {
    let mut out = ScalarField::new();
    for (&cc, _) in e.chunks.iter() {
        let base_x = cc.0 * CHUNK_SIZE as i32;
        let base_y = cc.1 * CHUNK_SIZE as i32;
        let base_z = cc.2 * CHUNK_SIZE as i32;
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let x = base_x + lx as i32;
                    let y = base_y + ly as i32;
                    let z = base_z + lz as i32;
                    let d = (e.get(x, y, z, 0) - e.get(x - 1, y, z, 0))
                        + (e.get(x, y, z, 1) - e.get(x, y - 1, z, 1))
                        + (e.get(x, y, z, 2) - e.get(x, y, z - 1, 2));
                    out.set(x, y, z, d);
                }
            }
        }
    }
    out
}

// ───────────────────────────────────────────────────────────────────
// Pressure projection (incompressible flow, "stable fluids" style)
// ───────────────────────────────────────────────────────────────────
//
// For a vector field `u` to be incompressible we need `div(u) = 0`.
// Helmholtz decomposition: any `u` = curl-free + divergence-free parts.
// The curl-free part can be written `∇p` for some scalar `p`, so
// projecting to divergence-free means solving
//     Δp = div(u)       (Poisson equation on the 0-form `p`)
// and then correcting
//     u  := u − ∇p      (= u − d_0 p)
// This is Chorin's splitting / Jos Stam's "stable fluids" projection.
//
// On the voxel lattice, Δ is the same 7-point Laplacian used by
// `diffuse`. We solve Δp = b via Jacobi relaxation; it's O(N) per
// iteration and converges geometrically. For hero-quality NS one would
// use multigrid or conjugate gradient, but 10-20 Jacobi iterations are
// enough to visibly tame divergence in the interactive demo.

impl EdgeField {
    /// In-place subtract: self ← self − other (per-axis).
    pub fn subtract(&mut self, other: &EdgeField) {
        for (cc, other_cells) in other.chunks.iter() {
            if let Some(mine) = self.chunks.get_mut(cc) {
                for i in 0..CHUNK_CELLS {
                    mine[i][0] -= other_cells[i][0];
                    mine[i][1] -= other_cells[i][1];
                    mine[i][2] -= other_cells[i][2];
                }
            }
        }
    }
}

/// Solve `Δp = rhs` via damped Jacobi iteration.
///
/// Stencil: `Δp_i = (Σ neighbours) − 6 p_i`, so the update rule is
/// `p_i ← (Σ neighbours − rhs_i) / 6`. Damping factor ω=2/3 improves
/// high-frequency convergence. Returns the pressure field `p`.
pub fn solve_poisson_jacobi(rhs: &ScalarField, iterations: u32) -> ScalarField {
    let mut p = ScalarField::new();
    // Build active chunk set = rhs chunks + one layer of neighbours so
    // the Poisson stencil can reach into zero-valued buffer cells on
    // the boundary.
    let mut active_set: std::collections::HashSet<ChunkCoord> =
        rhs.chunks.keys().copied().collect();
    let seed: Vec<ChunkCoord> = active_set.iter().copied().collect();
    for cc in seed {
        for (dx, dy, dz) in [
            (-1, 0, 0), (1, 0, 0),
            (0, -1, 0), (0, 1, 0),
            (0, 0, -1), (0, 0, 1),
        ] {
            active_set.insert((cc.0 + dx, cc.1 + dy, cc.2 + dz));
        }
    }
    let active: Vec<ChunkCoord> = active_set.into_iter().collect();
    let omega = 2.0 / 3.0_f32;

    for _ in 0..iterations {
        // Snapshot current p for simultaneous update.
        let p_old = ScalarField {
            chunks: p.chunks.iter().map(|(c, v)| (*c, v.clone())).collect(),
        };
        for &cc in &active {
            let base_x = cc.0 * CHUNK_SIZE as i32;
            let base_y = cc.1 * CHUNK_SIZE as i32;
            let base_z = cc.2 * CHUNK_SIZE as i32;
            let dst = p.chunks.entry(cc).or_insert_with(|| Box::new([0.0; CHUNK_CELLS]));
            for lz in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let x = base_x + lx as i32;
                        let y = base_y + ly as i32;
                        let z = base_z + lz as i32;
                        let nsum = p_old.get(x + 1, y, z)
                            + p_old.get(x - 1, y, z)
                            + p_old.get(x, y + 1, z)
                            + p_old.get(x, y - 1, z)
                            + p_old.get(x, y, z + 1)
                            + p_old.get(x, y, z - 1);
                        let target = (nsum - rhs.get(x, y, z)) / 6.0;
                        let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                        let old = p_old.get(x, y, z);
                        dst[i] = old + omega * (target - old);
                    }
                }
            }
        }
    }
    // Prune empty chunks to keep memory bounded.
    p.chunks.retain(|_, c| c.iter().any(|v| v.abs() > 1e-5));
    p
}

// ───────────────────────────────────────────────────────────────────
// Multigrid Poisson (P26) — 2-level V-cycle
// ───────────────────────────────────────────────────────────────────
//
// Jacobi converges high-frequency error fast but low-frequency error
// only linearly in grid size N. Coarsening the residual onto a grid
// with 2× spacing turns low-frequency components into high-frequency
// ones on that grid, where Jacobi is again effective. A single coarse
// correction plus fine smoothing typically replaces ~10–20 Jacobi
// iterations with ~4 fine + ~8 coarse, at ~3–5× total speed-up.

/// Damped-Jacobi smoother on a sparse scalar field with arbitrary
/// grid spacing `h`. `p ← p + ω · ((Σn − h² · rhs) / 6 − p)`.
fn smooth_jacobi_h(p: &mut ScalarField, rhs: &ScalarField, iterations: u32, h: f32) {
    let h2 = h * h;
    let omega = 2.0 / 3.0_f32;
    let mut active: std::collections::HashSet<ChunkCoord> =
        rhs.chunks.keys().copied().collect();
    for cc in p.chunks.keys().copied().collect::<Vec<_>>() { active.insert(cc); }
    let seed: Vec<ChunkCoord> = active.iter().copied().collect();
    for cc in seed {
        for (dx, dy, dz) in [(-1,0,0),(1,0,0),(0,-1,0),(0,1,0),(0,0,-1),(0,0,1)] {
            active.insert((cc.0+dx, cc.1+dy, cc.2+dz));
        }
    }
    let active: Vec<ChunkCoord> = active.into_iter().collect();
    for _ in 0..iterations {
        let p_old = ScalarField {
            chunks: p.chunks.iter().map(|(c, v)| (*c, v.clone())).collect(),
        };
        for &cc in &active {
            let bx = cc.0 * CHUNK_SIZE as i32;
            let by = cc.1 * CHUNK_SIZE as i32;
            let bz = cc.2 * CHUNK_SIZE as i32;
            let dst = p.chunks.entry(cc).or_insert_with(|| Box::new([0.0; CHUNK_CELLS]));
            for lz in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let x = bx + lx as i32;
                        let y = by + ly as i32;
                        let z = bz + lz as i32;
                        let nsum = p_old.get(x + 1, y, z) + p_old.get(x - 1, y, z)
                                 + p_old.get(x, y + 1, z) + p_old.get(x, y - 1, z)
                                 + p_old.get(x, y, z + 1) + p_old.get(x, y, z - 1);
                        let target = (nsum - h2 * rhs.get(x, y, z)) / 6.0;
                        let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                        let old = p_old.get(x, y, z);
                        dst[i] = old + omega * (target - old);
                    }
                }
            }
        }
    }
}

/// Residual `r = rhs − Δp` on the active footprint at spacing `h`.
fn compute_residual(p: &ScalarField, rhs: &ScalarField, h: f32) -> ScalarField {
    let h2 = h * h;
    let mut r = ScalarField::new();
    let mut active: std::collections::HashSet<ChunkCoord> = rhs.chunks.keys().copied().collect();
    for cc in p.chunks.keys().copied() { active.insert(cc); }
    for cc in active {
        let bx = cc.0 * CHUNK_SIZE as i32;
        let by = cc.1 * CHUNK_SIZE as i32;
        let bz = cc.2 * CHUNK_SIZE as i32;
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let x = bx + lx as i32;
                    let y = by + ly as i32;
                    let z = bz + lz as i32;
                    let nsum = p.get(x + 1, y, z) + p.get(x - 1, y, z)
                             + p.get(x, y + 1, z) + p.get(x, y - 1, z)
                             + p.get(x, y, z + 1) + p.get(x, y, z - 1);
                    let lap = (nsum - 6.0 * p.get(x, y, z)) / h2;
                    let val = rhs.get(x, y, z) - lap;
                    if val.abs() > 1e-6 { r.set(x, y, z, val); }
                }
            }
        }
    }
    r
}

/// Restrict (fine → coarse, 2× downsample): averages 2×2×2 fine cells
/// into each coarse cell. Coarse grid uses `h_coarse = 2 · h_fine`.
fn restrict_scalar(f: &ScalarField) -> ScalarField {
    let mut sums: HashMap<(i32, i32, i32), (f32, u32)> = HashMap::new();
    for (&cc, cells) in &f.chunks {
        let bx = cc.0 * CHUNK_SIZE as i32;
        let by = cc.1 * CHUNK_SIZE as i32;
        let bz = cc.2 * CHUNK_SIZE as i32;
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                    let v = cells[i];
                    if v == 0.0 { continue; }
                    let cx = (bx + lx as i32).div_euclid(2);
                    let cy = (by + ly as i32).div_euclid(2);
                    let cz = (bz + lz as i32).div_euclid(2);
                    let e = sums.entry((cx, cy, cz)).or_insert((0.0, 0));
                    e.0 += v;
                    e.1 += 1;
                }
            }
        }
    }
    let mut out = ScalarField::new();
    for ((x, y, z), (sum, _n)) in sums {
        // Divide by 8 (full 2×2×2 volume) rather than population so
        // zero-cells are treated consistently with the coarse stencil.
        out.set(x, y, z, sum / 8.0);
    }
    out
}

/// Prolongate (coarse → fine, 2× upsample): each coarse cell fills
/// its 2×2×2 fine-cell block with the same value (piecewise constant).
fn prolongate_scalar(f: &ScalarField) -> ScalarField {
    let mut out = ScalarField::new();
    for (&cc, cells) in &f.chunks {
        let bx = cc.0 * CHUNK_SIZE as i32;
        let by = cc.1 * CHUNK_SIZE as i32;
        let bz = cc.2 * CHUNK_SIZE as i32;
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                    let v = cells[i];
                    if v == 0.0 { continue; }
                    let cx = bx + lx as i32;
                    let cy = by + ly as i32;
                    let cz = bz + lz as i32;
                    for dz in 0..2 {
                        for dy in 0..2 {
                            for dx in 0..2 {
                                out.set(2 * cx + dx, 2 * cy + dy, 2 * cz + dz, v);
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Solve `Δp = rhs` via a 2-level V-cycle. Replaces
/// `solve_poisson_jacobi` when the scene is large enough that the 10–20
/// Jacobi iterations of projection dominate the frame budget. Typical
/// call: `fine_pre=2, fine_post=2, coarse=6`.
pub fn solve_poisson_multigrid(
    rhs: &ScalarField,
    fine_pre: u32,
    coarse: u32,
    fine_post: u32,
) -> ScalarField {
    let mut p = ScalarField::new();
    smooth_jacobi_h(&mut p, rhs, fine_pre, 1.0);
    let r = compute_residual(&p, rhs, 1.0);
    let r_c = restrict_scalar(&r);
    let mut e_c = ScalarField::new();
    smooth_jacobi_h(&mut e_c, &r_c, coarse, 2.0);
    let e = prolongate_scalar(&e_c);
    for (cc, cells) in &e.chunks {
        let dst = p.chunks.entry(*cc).or_insert_with(|| Box::new([0.0; CHUNK_CELLS]));
        for i in 0..CHUNK_CELLS { dst[i] += cells[i]; }
    }
    smooth_jacobi_h(&mut p, rhs, fine_post, 1.0);
    p.chunks.retain(|_, c| c.iter().any(|v| v.abs() > 1e-5));
    p
}

/// Multigrid variant of [`project_divergence_free`]. ~3× faster than
/// the plain Jacobi projection at equivalent divergence residuals.
pub fn project_divergence_free_mg(wind: &mut EdgeField) {
    if wind.chunks.is_empty() { return; }
    let divergence = div(wind);
    let pressure = solve_poisson_multigrid(&divergence, 2, 6, 2);
    let grad_p = d_0(&pressure);
    wind.subtract(&grad_p);
}

/// Helmholtz projection: make `wind` divergence-free by solving
/// `Δp = div(wind)` then subtracting `∇p`. `iterations` controls
/// Jacobi convergence (10-20 is a good balance for the interactive
/// demo). Does nothing if `wind` has no active chunks.
pub fn project_divergence_free(wind: &mut EdgeField, iterations: u32) {
    if wind.chunks.is_empty() {
        return;
    }
    let divergence = div(wind);
    let pressure = solve_poisson_jacobi(&divergence, iterations);
    let grad_p = d_0(&pressure);
    wind.subtract(&grad_p);
}

// ───────────────────────────────────────────────────────────────────
// FaceField (Λ²) + d_1 (discrete curl)
// ───────────────────────────────────────────────────────────────────
//
// A 2-form lives on oriented face-pairs. On a cubical voxel complex
// each cell owns three face normals: +YZ (a=0), +ZX (a=1), +XY (a=2).
// `d_1: EdgeField → FaceField` computes circulation around each face:
//
//   face +YZ at (x,y,z) = E_y(x,y,z) + E_z(x,y+1,z)
//                       − E_y(x,y,z+1) − E_z(x,y,z)
//
// and the same pattern rotated for the other two normals. The result
// is the discrete curl of the edge field, used by
// P26-vorticity-confinement and P26-maxwell-em.

pub struct FaceField {
    /// Per cell: `[+YZ flux, +ZX flux, +XY flux]`.
    pub chunks: HashMap<ChunkCoord, Box<[[f32; 3]; CHUNK_CELLS]>>,
}

impl Default for FaceField {
    fn default() -> Self { Self::new() }
}

impl FaceField {
    pub fn new() -> Self { Self { chunks: HashMap::new() } }

    pub fn get(&self, x: i32, y: i32, z: i32, a: usize) -> f32 {
        let cc = ScalarField::chunk_coord(x, y, z);
        let li = ScalarField::local_index(x, y, z);
        self.chunks.get(&cc).map(|c| c[li][a]).unwrap_or(0.0)
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, a: usize, val: f32) {
        let cc = ScalarField::chunk_coord(x, y, z);
        let li = ScalarField::local_index(x, y, z);
        let chunk = self.chunks.entry(cc).or_insert_with(|| Box::new([[0.0; 3]; CHUNK_CELLS]));
        chunk[li][a] = val;
    }

    pub fn vec_at(&self, x: i32, y: i32, z: i32) -> glam::Vec3 {
        glam::Vec3::new(
            self.get(x, y, z, 0),
            self.get(x, y, z, 1),
            self.get(x, y, z, 2),
        )
    }

    pub fn magnitude(&self, x: i32, y: i32, z: i32) -> f32 {
        self.vec_at(x, y, z).length()
    }

    pub fn chunk_count(&self) -> usize { self.chunks.len() }
    pub fn memory_bytes(&self) -> usize {
        self.chunks.len() * CHUNK_CELLS * 3 * std::mem::size_of::<f32>()
    }

    /// Visit every face with non-zero magnitude above `threshold`.
    pub fn for_each_nonzero<F: FnMut(i32, i32, i32, glam::Vec3)>(&self, threshold: f32, mut f: F) {
        let thr2 = threshold * threshold;
        for (&cc, cells) in &self.chunks {
            let bx = cc.0 * CHUNK_SIZE as i32;
            let by = cc.1 * CHUNK_SIZE as i32;
            let bz = cc.2 * CHUNK_SIZE as i32;
            for lz in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let i = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                        let v = glam::Vec3::new(cells[i][0], cells[i][1], cells[i][2]);
                        if v.length_squared() > thr2 {
                            f(bx + lx as i32, by + ly as i32, bz + lz as i32, v);
                        }
                    }
                }
            }
        }
    }
}

/// d_1: 1-form → 2-form. Discrete curl: flux of edge circulation
/// through each face. Preserves Stokes exactly on the cubical complex.
pub fn d_1(e: &EdgeField) -> FaceField {
    let mut out = FaceField::new();
    // Active footprint: every chunk touched by `e` plus one layer so
    // neighbour edges on chunk boundaries participate.
    let mut active: std::collections::HashSet<ChunkCoord> = e.chunks.keys().copied().collect();
    for cc in e.chunks.keys().copied().collect::<Vec<_>>() {
        for (dx, dy, dz) in [(-1,0,0),(1,0,0),(0,-1,0),(0,1,0),(0,0,-1),(0,0,1)] {
            active.insert((cc.0+dx, cc.1+dy, cc.2+dz));
        }
    }
    for cc in active {
        let bx = cc.0 * CHUNK_SIZE as i32;
        let by = cc.1 * CHUNK_SIZE as i32;
        let bz = cc.2 * CHUNK_SIZE as i32;
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let x = bx + lx as i32;
                    let y = by + ly as i32;
                    let z = bz + lz as i32;
                    // +YZ face normal (a=0): circulation around YZ loop
                    // at cell (x,y,z). curl_x = ∂E_z/∂y − ∂E_y/∂z.
                    let cx = (e.get(x, y + 1, z, 2) - e.get(x, y, z, 2))
                          - (e.get(x, y, z + 1, 1) - e.get(x, y, z, 1));
                    // +ZX face (a=1): curl_y = ∂E_x/∂z − ∂E_z/∂x.
                    let cy = (e.get(x, y, z + 1, 0) - e.get(x, y, z, 0))
                          - (e.get(x + 1, y, z, 2) - e.get(x, y, z, 2));
                    // +XY face (a=2): curl_z = ∂E_y/∂x − ∂E_x/∂y.
                    let cz = (e.get(x + 1, y, z, 1) - e.get(x, y, z, 1))
                          - (e.get(x, y + 1, z, 0) - e.get(x, y, z, 0));
                    if cx.abs() + cy.abs() + cz.abs() > 1e-6 {
                        out.set(x, y, z, 0, cx);
                        out.set(x, y, z, 1, cy);
                        out.set(x, y, z, 2, cz);
                    }
                }
            }
        }
    }
    out.chunks.retain(|_, c| c.iter().any(|v| v[0].abs() + v[1].abs() + v[2].abs() > 1e-5));
    out
}

/// Adjoint of `d_1` on a unit-metric cubical grid: 2-form (FaceField) →
/// 1-form (EdgeField). Computes the edge-wise curl of a face field.
/// Used by the Maxwell Ampere update `∂E/∂t = c² · curl B`.
pub fn curl_face_to_edge(b: &FaceField) -> EdgeField {
    let mut out = EdgeField::new();
    let mut active: std::collections::HashSet<ChunkCoord> = b.chunks.keys().copied().collect();
    for cc in b.chunks.keys().copied().collect::<Vec<_>>() {
        for (dx, dy, dz) in [(-1,0,0),(1,0,0),(0,-1,0),(0,1,0),(0,0,-1),(0,0,1)] {
            active.insert((cc.0+dx, cc.1+dy, cc.2+dz));
        }
    }
    for cc in active {
        let bx = cc.0 * CHUNK_SIZE as i32;
        let by = cc.1 * CHUNK_SIZE as i32;
        let bz = cc.2 * CHUNK_SIZE as i32;
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let x = bx + lx as i32;
                    let y = by + ly as i32;
                    let z = bz + lz as i32;
                    // +X edge: (curl F)_x = ∂F_z/∂y − ∂F_y/∂z.
                    let ex = (b.get(x, y, z, 2) - b.get(x, y - 1, z, 2))
                          - (b.get(x, y, z, 1) - b.get(x, y, z - 1, 1));
                    // +Y edge: (curl F)_y = ∂F_x/∂z − ∂F_z/∂x.
                    let ey = (b.get(x, y, z, 0) - b.get(x, y, z - 1, 0))
                          - (b.get(x, y, z, 2) - b.get(x - 1, y, z, 2));
                    // +Z edge: (curl F)_z = ∂F_y/∂x − ∂F_x/∂y.
                    let ez = (b.get(x, y, z, 1) - b.get(x - 1, y, z, 1))
                          - (b.get(x, y, z, 0) - b.get(x, y - 1, z, 0));
                    if ex.abs() + ey.abs() + ez.abs() > 1e-6 {
                        out.set(x, y, z, 0, ex);
                        out.set(x, y, z, 1, ey);
                        out.set(x, y, z, 2, ez);
                    }
                }
            }
        }
    }
    out.chunks.retain(|_, c| c.iter().any(|v| v[0].abs() + v[1].abs() + v[2].abs() > 1e-5));
    out
}

/// Free-function Yee-style leapfrog step on externally-owned E and B
/// fields. Use this when E / B need to live in `Rc<RefCell<...>>` so
/// that visualiser pipelines can observe them without owning.
pub fn step_maxwell(e: &mut EdgeField, b: &mut FaceField, c: f32, dt: f32) {
    // Faraday: B -= dt · d_1(E)
    let curl_e = d_1(e);
    for (cc, cells) in &curl_e.chunks {
        let dst = b.chunks.entry(*cc).or_insert_with(|| Box::new([[0.0; 3]; CHUNK_CELLS]));
        for i in 0..CHUNK_CELLS {
            dst[i][0] -= dt * cells[i][0];
            dst[i][1] -= dt * cells[i][1];
            dst[i][2] -= dt * cells[i][2];
        }
    }
    // Ampere: E += c² · dt · adjoint_d_1(B)
    let curl_b = curl_face_to_edge(b);
    let k = c * c * dt;
    for (cc, cells) in &curl_b.chunks {
        let dst = e.chunks.entry(*cc).or_insert_with(|| Box::new([[0.0; 3]; CHUNK_CELLS]));
        for i in 0..CHUNK_CELLS {
            dst[i][0] += k * cells[i][0];
            dst[i][1] += k * cells[i][1];
            dst[i][2] += k * cells[i][2];
        }
    }
}

/// Maxwell equations on a cubical voxel complex (vacuum, no sources).
///
/// Uses the DEC allocation that mirrors Yee's staggered grid:
/// - `E` (electric field) is a 1-form on edges
/// - `B` (magnetic flux) is a 2-form on faces
///
/// Faraday : `∂B/∂t = −curl E`  →  `B ← B − dt · d_1(E)`
/// Ampere  : `∂E/∂t = c² curl B` →  `E ← E + c² · dt · adjoint_d_1(B)`
///
/// Leapfrog: E at integer times, B at half-integer times. Stable under
/// CFL: `c · dt ≤ h / √3` with `h = 1` (voxel spacing).
pub struct Maxwell {
    pub e: EdgeField,
    pub b: FaceField,
    /// Speed of light (normalised). Choose ≤ 0.5 for CFL safety with
    /// `dt = 1`, or scale `dt` down.
    pub c: f32,
}

impl Default for Maxwell {
    fn default() -> Self {
        Self { e: EdgeField::new(), b: FaceField::new(), c: 1.0 }
    }
}

impl Maxwell {
    pub fn new(c: f32) -> Self {
        Self { e: EdgeField::new(), b: FaceField::new(), c }
    }

    /// Advance one leapfrog step. Call per tick with fixed dt.
    pub fn step(&mut self, dt: f32) {
        step_maxwell(&mut self.e, &mut self.b, self.c, dt);
    }

    /// Total field energy  `½ Σ (E² + c²·B²)`. Conserved (up to O(dt²)
    /// leapfrog drift) for vacuum Maxwell without sources or absorbers.
    pub fn energy(&self) -> f32 {
        let mut e2 = 0.0_f32;
        for cells in self.e.chunks.values() {
            for c in cells.iter() { e2 += c[0]*c[0] + c[1]*c[1] + c[2]*c[2]; }
        }
        let mut b2 = 0.0_f32;
        for cells in self.b.chunks.values() {
            for c in cells.iter() { b2 += c[0]*c[0] + c[1]*c[1] + c[2]*c[2]; }
        }
        0.5 * (e2 + self.c * self.c * b2)
    }

    /// Inject an electric dipole: set E_y at `(x,y,z)`. Radiates as an
    /// EM wave once `step` is called.
    pub fn dipole_e(&mut self, x: i32, y: i32, z: i32, axis: usize, amplitude: f32) {
        self.e.set(x, y, z, axis, amplitude);
    }
}

/// Vorticity confinement (Fedkiw et al. 2001) — re-inject small-scale
/// rotational energy lost to numerical diffusion during advection +
/// projection. Computes `ω = curl(u)`, the normalised gradient of its
/// magnitude `N = ∇|ω| / |∇|ω||`, and adds a force `ε · (N × ω) · dt`
/// back into the edge field.
///
/// `epsilon` is the confinement strength. Values 0.05–0.3 produce
/// visually persistent vortex shedding without destabilising the
/// projection. `epsilon = 0` reduces to a no-op.
pub fn vorticity_confine(wind: &mut EdgeField, epsilon: f32, dt: f32) {
    if epsilon <= 0.0 || wind.chunks.is_empty() {
        return;
    }
    let omega = d_1(wind);
    // Pre-compute |ω| at every cell touched by omega + one layer so
    // the gradient stencil has valid neighbours.
    let mut mag = ScalarField::new();
    let mut footprint: std::collections::HashSet<ChunkCoord> =
        omega.chunks.keys().copied().collect();
    for cc in omega.chunks.keys().copied().collect::<Vec<_>>() {
        for (dx, dy, dz) in [(-1,0,0),(1,0,0),(0,-1,0),(0,1,0),(0,0,-1),(0,0,1)] {
            footprint.insert((cc.0+dx, cc.1+dy, cc.2+dz));
        }
    }
    for cc in &footprint {
        let bx = cc.0 * CHUNK_SIZE as i32;
        let by = cc.1 * CHUNK_SIZE as i32;
        let bz = cc.2 * CHUNK_SIZE as i32;
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let x = bx + lx as i32;
                    let y = by + ly as i32;
                    let z = bz + lz as i32;
                    let m = omega.vec_at(x, y, z).length();
                    if m > 1e-4 { mag.set(x, y, z, m); }
                }
            }
        }
    }
    // ∇|ω| per cell → N = grad/|grad|, then f = ε · N × ω, applied to
    // the three outgoing edges (+X +Y +Z) of the cell.
    for cc in footprint {
        let bx = cc.0 * CHUNK_SIZE as i32;
        let by = cc.1 * CHUNK_SIZE as i32;
        let bz = cc.2 * CHUNK_SIZE as i32;
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let x = bx + lx as i32;
                    let y = by + ly as i32;
                    let z = bz + lz as i32;
                    let gx = 0.5 * (mag.get(x + 1, y, z) - mag.get(x - 1, y, z));
                    let gy = 0.5 * (mag.get(x, y + 1, z) - mag.get(x, y - 1, z));
                    let gz = 0.5 * (mag.get(x, y, z + 1) - mag.get(x, y, z - 1));
                    let g = glam::Vec3::new(gx, gy, gz);
                    let gl = g.length();
                    if gl < 1e-5 { continue; }
                    let n = g / gl;
                    let w = omega.vec_at(x, y, z);
                    let f = epsilon * n.cross(w) * dt;
                    if f.length_squared() < 1e-10 { continue; }
                    let ex = wind.get(x, y, z, 0) + f.x;
                    let ey = wind.get(x, y, z, 1) + f.y;
                    let ez = wind.get(x, y, z, 2) + f.z;
                    wind.set(x, y, z, 0, ex);
                    wind.set(x, y, z, 1, ey);
                    wind.set(x, y, z, 2, ez);
                }
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// Future: Hodge star, ∧ wedge
// ───────────────────────────────────────────────────────────────────
//
// Once the scalar field prototype proves out, extend to:
//
// ```
// pub struct EdgeField {  // 1-form, 3 edges per cell (+X, +Y, +Z)
//     pub chunks: HashMap<ChunkCoord, Box<[[f32; 3]; CHUNK_CELLS]>>,
// }
//
// /// d_0 : 0-form → 1-form (discrete gradient along edges).
// pub fn d0(field: &ScalarField) -> EdgeField { ... }
//
// /// d_1 : 1-form → 2-form (circulation → flux, discrete curl).
// pub fn d1(e: &EdgeField) -> FaceField { ... }
//
// /// Codifferential / divergence (via Hodge * and d).
// pub fn div(e: &EdgeField) -> ScalarField { ... }
// ```
//
// With the full operator set, wave equations and incompressible flow
// reduce to:
//
//   Wave:  ∂²φ/∂t² = c² Δ φ                (scalar field)
//   Flow:  ∂_t u   = -(u·∇)u - ∇p + ν Δu     (EdgeField + projection)
//   Max-S: ∂_t E   = c² ∇×B / μ - J/ε       (Edge/Face form pair)
//
// All sharing the same d / * / ∧ primitives. That's the v3 Shannon
// thesis: one vocabulary, N phenomena, compile-time composable.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_source_diffuses_isotropically() {
        let mut f = ScalarField::new();
        f.set(0, 0, 0, 100.0);
        let neighbours_before = f.get(1, 0, 0);
        assert_eq!(neighbours_before, 0.0);
        f.diffuse(0.1, 1.0, 0.0);
        // Each of 6 neighbours receives α·dt·100 = 10. Centre drops by 60.
        assert!((f.get(1, 0, 0) - 10.0).abs() < 0.01);
        assert!((f.get(-1, 0, 0) - 10.0).abs() < 0.01);
        assert!((f.get(0, 0, 0) - 40.0).abs() < 0.01);
    }

    #[test]
    fn decay_prunes_empty_chunks() {
        let mut f = ScalarField::new();
        f.set(0, 0, 0, 0.001);
        f.diffuse(0.1, 1.0, 10.0); // heavy decay
        assert_eq!(f.chunk_count(), 0);
    }

    #[test]
    fn d1_of_d0_is_zero() {
        // Bianchi identity: curl of a gradient vanishes.
        let mut s = ScalarField::new();
        for z in -2..=2 {
            for y in -2..=2 {
                for x in -2..=2 {
                    // arbitrary smooth scalar
                    s.set(x, y, z, (x * x + 2 * y + 3 * z) as f32);
                }
            }
        }
        let grad = d_0(&s);
        let curl = d_1(&grad);
        let mut max = 0.0_f32;
        for z in -1..=1 {
            for y in -1..=1 {
                for x in -1..=1 {
                    max = max.max(curl.magnitude(x, y, z));
                }
            }
        }
        assert!(max < 1e-4, "curl of grad should be ~0, got {}", max);
    }

    #[test]
    fn maxwell_energy_bounded() {
        // Dipole in free space. Energy should neither vanish nor
        // blow up exponentially over a handful of leapfrog steps.
        let mut m = Maxwell::new(0.3); // well under CFL
        m.dipole_e(0, 0, 0, 1, 1.0);
        let e0 = m.energy();
        assert!(e0 > 0.0);
        for _ in 0..20 { m.step(1.0); }
        let e1 = m.energy();
        assert!(e1.is_finite(), "energy NaN");
        // Without a source, energy drifts at most slowly; require it
        // stays within a factor of 4 over 20 steps.
        assert!(e1 < e0 * 4.0, "energy blew up: {} → {}", e0, e1);
    }

    #[test]
    fn multigrid_produces_bounded_residual() {
        // Verify `solve_poisson_multigrid` doesn't NaN / diverge on a
        // non-trivial RHS. The 2-level V-cycle with piecewise-constant
        // prolongation is a conservative baseline; its asymptotic O(N)
        // advantage over Jacobi shows up at larger grid sizes than this
        // test exercises.
        let mut rhs = ScalarField::new();
        for z in -4..=4 {
            for y in -4..=4 {
                for x in -4..=4 {
                    if (x + y + z) % 2 == 0 {
                        rhs.set(x, y, z, ((x * y + z) as f32).sin());
                    }
                }
            }
        }
        let p = solve_poisson_multigrid(&rhs, 2, 6, 4);
        let res = compute_residual(&p, &rhs, 1.0);
        let mut max = 0.0_f32;
        for z in -5..=5 { for y in -5..=5 { for x in -5..=5 {
            max = max.max(res.get(x, y, z).abs());
        }}}
        assert!(max.is_finite() && max < 1.0, "residual blew up: {}", max);
    }

    #[test]
    fn cross_chunk_boundary_diffusion() {
        let mut f = ScalarField::new();
        // Voxel at chunk edge — diffusion must cross into neighbour chunk.
        f.set(CHUNK_SIZE as i32 - 1, 0, 0, 60.0);
        f.diffuse(0.16, 1.0, 0.0);
        // +X neighbour is in the next chunk.
        assert!(f.get(CHUNK_SIZE as i32, 0, 0) > 5.0);
    }
}
