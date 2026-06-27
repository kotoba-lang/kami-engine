//! MetaHuman DNA file parser — reads Epic Games .dna binary format.
//!
//! Binary layout (reverse-engineered from dnacalib C++ source):
//!   - **Big-endian** (network byte order)
//!   - **u32 length-prefixed** arrays (count then elements)
//!   - **SoA** (Structure-of-Arrays) for vector data (all Xs, then Ys, then Zs)
//!   - **ArchiveOffset** for section seeking (u32 absolute byte offsets)
//!   - **Vertex layout indirection**: faces → layouts → positions/UVs/normals
//!
//! File structure:
//!   `DNA` magic (3B) + generation (u16) + version (u16)
//!   + SectionLookupTable (8 × u32 offsets)
//!   + Descriptor + Definition + Behavior + Geometry
//!   + `AND` EOF marker (3B)
//!
//! Reference: <https://github.com/EpicGames/MetaHuman-DNA-Calibration>

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Scale factor: DNA stores in centimeters, KAMI uses meters.
const CM_TO_M: f32 = 0.01;

// ─── Data Structures ───

/// Complete parsed DNA file.
#[derive(Debug, Clone)]
pub struct DnaFile {
    pub header: DnaHeader,
    pub descriptor: DnaDescriptor,
    pub definition: DnaDefinition,
    pub geometry: DnaGeometry,
}

/// File header.
#[derive(Debug, Clone)]
pub struct DnaHeader {
    pub generation: u16,
    pub version: u16,
    pub section_offsets: SectionOffsets,
}

/// Section byte offsets.
#[derive(Debug, Clone, Copy)]
pub struct SectionOffsets {
    pub descriptor: u32,
    pub definition: u32,
    pub behavior: u32,
    pub controls: u32,
    pub joints: u32,
    pub blend_shape_channels: u32,
    pub animated_maps: u32,
    pub geometry: u32,
}

/// Descriptor — character metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnaDescriptor {
    pub name: String,
    pub archetype: u16,
    pub gender: u16,
    pub age: u16,
    pub metadata: Vec<(String, String)>,
    pub translation_unit: u16,
    pub rotation_unit: u16,
    pub coordinate_system: [u16; 3],
    pub lod_count: u16,
    pub max_lod: u16,
    pub complexity: String,
    pub db_name: String,
}

/// Definition — static rig structure.
#[derive(Debug, Clone)]
pub struct DnaDefinition {
    pub joint_names: Vec<String>,
    pub joint_hierarchy: Vec<u16>,
    pub neutral_joint_translations: SoaVec3,
    pub neutral_joint_rotations: SoaVec3,
    pub blend_shape_channel_names: Vec<String>,
    pub animated_map_names: Vec<String>,
    pub mesh_names: Vec<String>,
    pub gui_control_names: Vec<String>,
    pub raw_control_names: Vec<String>,
}

/// Structure-of-Arrays Vec3: separate X, Y, Z arrays.
#[derive(Debug, Clone)]
pub struct SoaVec3 {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    pub zs: Vec<f32>,
}

impl SoaVec3 {
    /// Get the i-th vector.
    pub fn get(&self, i: usize) -> Vec3 {
        Vec3::new(
            self.xs.get(i).copied().unwrap_or(0.0),
            self.ys.get(i).copied().unwrap_or(0.0),
            self.zs.get(i).copied().unwrap_or(0.0),
        )
    }

    /// Number of vectors.
    pub fn len(&self) -> usize {
        self.xs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.xs.is_empty()
    }
}

/// Geometry layer — all meshes.
#[derive(Debug, Clone)]
pub struct DnaGeometry {
    pub meshes: Vec<DnaMesh>,
}

/// A single mesh from the geometry section.
#[derive(Debug, Clone)]
pub struct DnaMesh {
    /// Vertex positions (SoA, centimeters in DNA, converted to meters).
    pub positions: SoaVec3,
    /// Texture coordinates (SoA: Us and Vs).
    pub uvs_u: Vec<f32>,
    pub uvs_v: Vec<f32>,
    /// Vertex normals (SoA).
    pub normals: SoaVec3,
    /// Vertex layout: indices into positions/UVs/normals arrays.
    pub layout_positions: Vec<u32>,
    pub layout_uvs: Vec<u32>,
    pub layout_normals: Vec<u32>,
    /// Face data: each face is a list of layout indices.
    pub faces: Vec<Vec<u32>>,
    /// Max bone influences per vertex.
    pub max_influences: u16,
    /// Per-vertex skin weights: (joint_indices, weights).
    pub skin_weights: Vec<(Vec<u16>, Vec<f32>)>,
    /// Blend shape targets.
    pub blend_shapes: Vec<DnaBlendShape>,
}

/// Blend shape target.
#[derive(Debug, Clone)]
pub struct DnaBlendShape {
    pub channel_index: u16,
    pub vertex_indices: Vec<u32>,
    pub deltas: SoaVec3,
}

// ─── Big-Endian Binary Reader ───

struct BeReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BeReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn u8(&mut self) -> u8 {
        let v = self.data[self.pos];
        self.pos += 1;
        v
    }

    fn u16(&mut self) -> u16 {
        let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        v
    }

    fn u32(&mut self) -> u32 {
        let v = u32::from_be_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        v
    }

    fn f32(&mut self) -> f32 {
        let v = f32::from_be_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        v
    }

    /// Read length-prefixed string (u32 count + chars).
    fn string(&mut self) -> String {
        let len = self.u32() as usize;
        if len == 0 {
            return String::new();
        }
        let s = String::from_utf8_lossy(&self.data[self.pos..self.pos + len]).to_string();
        self.pos += len;
        s
    }

    /// Read u32-prefixed array of u16.
    fn array_u16(&mut self) -> Vec<u16> {
        let count = self.u32() as usize;
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push(self.u16());
        }
        v
    }

    /// Read u32-prefixed array of u32.
    fn array_u32(&mut self) -> Vec<u32> {
        let count = self.u32() as usize;
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push(self.u32());
        }
        v
    }

    /// Read u32-prefixed array of f32.
    fn array_f32(&mut self) -> Vec<f32> {
        let count = self.u32() as usize;
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push(self.f32());
        }
        v
    }

    /// Read u32-prefixed array of strings.
    fn array_string(&mut self) -> Vec<String> {
        let count = self.u32() as usize;
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push(self.string());
        }
        v
    }

    /// Read SoA Vec3 (3 separate f32 arrays).
    fn soa_vec3(&mut self) -> SoaVec3 {
        let xs = self.array_f32();
        let ys = self.array_f32();
        let zs = self.array_f32();
        SoaVec3 { xs, ys, zs }
    }

    /// Skip a u32-prefixed array of given element size.
    fn skip_array(&mut self, elem_size: usize) {
        let count = self.u32() as usize;
        self.pos += count * elem_size;
    }

    /// Skip a LOD mapping structure.
    fn skip_lod_mapping(&mut self) {
        // lods array
        let lods_count = self.u32() as usize;
        self.pos += lods_count * 2; // u16[]
                                    // indices matrix (outer count, then inner arrays)
        let outer = self.u32() as usize;
        for _ in 0..outer {
            let inner = self.u32() as usize;
            self.pos += inner * 2; // u16[]
        }
    }
}

// ─── Parser ───

impl DnaFile {
    /// Parse a .dna binary file (Epic Games MetaHuman format).
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 39 {
            return Err("DNA file too small".into());
        }

        let mut r = BeReader::new(data);

        // Magic: "DNA" (3 bytes)
        if r.u8() != b'D' || r.u8() != b'N' || r.u8() != b'A' {
            return Err("Invalid DNA magic".into());
        }

        // Header
        let generation = r.u16();
        let version = r.u16();
        if generation != 2 || version != 1 {
            return Err(format!(
                "Unsupported DNA version: gen={generation} ver={version}"
            ));
        }

        // Section lookup table (8 × u32 BE)
        let section_offsets = SectionOffsets {
            descriptor: r.u32(),
            definition: r.u32(),
            behavior: r.u32(),
            controls: r.u32(),
            joints: r.u32(),
            blend_shape_channels: r.u32(),
            animated_maps: r.u32(),
            geometry: r.u32(),
        };

        let header = DnaHeader {
            generation,
            version,
            section_offsets,
        };

        // Parse descriptor
        r.seek(section_offsets.descriptor as usize);
        let descriptor = Self::parse_descriptor(&mut r)?;

        // Parse definition
        r.seek(section_offsets.definition as usize);
        let definition = Self::parse_definition(&mut r)?;

        // Parse geometry (skip behavior for now — not needed for mesh display)
        r.seek(section_offsets.geometry as usize);
        let geometry = Self::parse_geometry(&mut r)?;

        Ok(Self {
            header,
            descriptor,
            definition,
            geometry,
        })
    }

    fn parse_descriptor(r: &mut BeReader) -> Result<DnaDescriptor, String> {
        let name = r.string();
        let archetype = r.u16();
        let gender = r.u16();
        let age = r.u16();

        // Metadata key-value pairs
        let meta_count = r.u32() as usize;
        let mut metadata = Vec::with_capacity(meta_count);
        for _ in 0..meta_count {
            let key = r.string();
            let val = r.string();
            metadata.push((key, val));
        }

        let translation_unit = r.u16();
        let rotation_unit = r.u16();
        let coordinate_system = [r.u16(), r.u16(), r.u16()];
        let lod_count = r.u16();
        let max_lod = r.u16();
        let complexity = r.string();
        let db_name = r.string();

        Ok(DnaDescriptor {
            name,
            archetype,
            gender,
            age,
            metadata,
            translation_unit,
            rotation_unit,
            coordinate_system,
            lod_count,
            max_lod,
            complexity,
            db_name,
        })
    }

    fn parse_definition(r: &mut BeReader) -> Result<DnaDefinition, String> {
        // 4 LOD mappings (joint, blendShape, animatedMap, mesh)
        for _ in 0..4 {
            r.skip_lod_mapping();
        }

        // Name arrays
        let gui_control_names = r.array_string();
        let raw_control_names = r.array_string();
        let joint_names = r.array_string();
        let blend_shape_channel_names = r.array_string();
        let animated_map_names = r.array_string();
        let mesh_names = r.array_string();

        // meshBlendShapeChannelMapping (surjective mapping: from + to)
        let _from = r.array_u16();
        let _to = r.array_u16();

        // Joint hierarchy (parent indices)
        let joint_hierarchy = r.array_u16();

        // Neutral joint transforms (SoA)
        let neutral_joint_translations = r.soa_vec3();
        let neutral_joint_rotations = r.soa_vec3();

        Ok(DnaDefinition {
            joint_names,
            joint_hierarchy,
            neutral_joint_translations,
            neutral_joint_rotations,
            blend_shape_channel_names,
            animated_map_names,
            mesh_names,
            gui_control_names,
            raw_control_names,
        })
    }

    fn parse_geometry(r: &mut BeReader) -> Result<DnaGeometry, String> {
        let mesh_count = r.u32() as usize;
        let mut meshes = Vec::with_capacity(mesh_count);

        // Each mesh: u32 end-offset, then mesh data sequentially
        for _ in 0..mesh_count {
            let _end_offset = r.u32(); // ArchiveOffset (backpatch target, skip)
                                       // Positions (SoA, centimeters)
            let mut positions = r.soa_vec3();
            // Convert cm → m
            for x in &mut positions.xs {
                *x *= CM_TO_M;
            }
            for y in &mut positions.ys {
                *y *= CM_TO_M;
            }
            for z in &mut positions.zs {
                *z *= CM_TO_M;
            }

            // UVs (SoA: Us and Vs)
            let uvs_u = r.array_f32();
            let uvs_v = r.array_f32();

            // Normals (SoA)
            let normals = r.soa_vec3();

            // Vertex layouts (indices into position/UV/normal arrays)
            let layout_positions = r.array_u32();
            let layout_uvs = r.array_u32();
            let layout_normals = r.array_u32();

            // Faces (each face: u32 count + u32[] layout indices)
            let face_count = r.u32() as usize;
            let mut faces = Vec::with_capacity(face_count);
            for _ in 0..face_count {
                let fv = r.array_u32();
                faces.push(fv);
            }

            // Skin weights
            let max_influences = r.u16();
            let sw_count = r.u32() as usize;
            let mut skin_weights = Vec::with_capacity(sw_count);
            for _ in 0..sw_count {
                let weights = r.array_f32();
                let joint_indices = r.array_u16();
                skin_weights.push((joint_indices, weights));
            }

            // Blend shape targets
            let bs_count = r.u32() as usize;
            let mut blend_shapes = Vec::with_capacity(bs_count);
            for _ in 0..bs_count {
                let mut deltas = r.soa_vec3();
                // Convert cm → m
                for x in &mut deltas.xs {
                    *x *= CM_TO_M;
                }
                for y in &mut deltas.ys {
                    *y *= CM_TO_M;
                }
                for z in &mut deltas.zs {
                    *z *= CM_TO_M;
                }
                let vertex_indices = r.array_u32();
                let channel_index = r.u16();
                blend_shapes.push(DnaBlendShape {
                    channel_index,
                    vertex_indices,
                    deltas,
                });
            }

            meshes.push(DnaMesh {
                positions,
                uvs_u,
                uvs_v,
                normals,
                layout_positions,
                layout_uvs,
                layout_normals,
                faces,
                max_influences,
                skin_weights,
                blend_shapes,
            });
        }

        Ok(DnaGeometry { meshes })
    }

    /// Convert DNA skeleton to `kami_skeleton::Skeleton`.
    pub fn to_skeleton(&self) -> kami_skeleton::Skeleton {
        let d = &self.definition;
        let bones = d
            .joint_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let parent = d.joint_hierarchy.get(i).copied().unwrap_or(0xFFFF);
                let t = d.neutral_joint_translations.get(i) * CM_TO_M;
                // Rotations in DNA are in degrees — convert to quaternion
                let r = d.neutral_joint_rotations.get(i);
                let rot = Quat::from_euler(
                    glam::EulerRot::XYZ,
                    r.x.to_radians(),
                    r.y.to_radians(),
                    r.z.to_radians(),
                );
                kami_skeleton::Bone {
                    name: name.clone(),
                    parent: if parent == 0xFFFF || parent as usize == i {
                        None
                    } else {
                        Some(parent as usize)
                    },
                    local_position: t.to_array(),
                    local_rotation: [rot.x, rot.y, rot.z, rot.w],
                    local_scale: [1.0, 1.0, 1.0],
                    inverse_bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
                }
            })
            .collect();
        kami_skeleton::Skeleton { bones }
    }

    /// Triangulate a mesh's faces and return (positions, normals, uvs, indices)
    /// with vertex layout indirection resolved.
    pub fn triangulate_mesh(&self, mesh_index: usize) -> TriangulatedMesh {
        let mesh = &self.geometry.meshes[mesh_index];
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();

        for face in &mesh.faces {
            if face.len() < 3 {
                continue;
            }
            // Fan triangulation for quads and n-gons
            let base = positions.len() as u32;
            for &layout_idx in face {
                let li = layout_idx as usize;
                let pi = mesh.layout_positions.get(li).copied().unwrap_or(0) as usize;
                let ui = mesh.layout_uvs.get(li).copied().unwrap_or(0) as usize;
                let ni = mesh.layout_normals.get(li).copied().unwrap_or(0) as usize;

                positions.push(mesh.positions.get(pi));
                normals.push(mesh.normals.get(ni));
                uvs.push([
                    mesh.uvs_u.get(ui).copied().unwrap_or(0.0),
                    mesh.uvs_v.get(ui).copied().unwrap_or(0.0),
                ]);
            }
            // Fan triangulation: 0-1-2, 0-2-3, 0-3-4, ...
            for i in 2..face.len() as u32 {
                indices.push(base);
                indices.push(base + i - 1);
                indices.push(base + i);
            }
        }

        TriangulatedMesh {
            positions,
            normals,
            uvs,
            indices,
        }
    }

    /// Get mesh name by index.
    pub fn mesh_name(&self, index: usize) -> &str {
        self.definition
            .mesh_names
            .get(index)
            .map(|s| s.as_str())
            .unwrap_or("unknown")
    }

    /// Total vertex count across all meshes (unique positions).
    pub fn total_vertices(&self) -> usize {
        self.geometry.meshes.iter().map(|m| m.positions.len()).sum()
    }

    /// Total face count.
    pub fn total_faces(&self) -> usize {
        self.geometry.meshes.iter().map(|m| m.faces.len()).sum()
    }

    /// Total blend shape count.
    pub fn total_blend_shapes(&self) -> usize {
        self.geometry
            .meshes
            .iter()
            .map(|m| m.blend_shapes.len())
            .sum()
    }
}

/// Triangulated mesh output (layout indirection resolved, quads → triangles).
#[derive(Debug, Clone)]
pub struct TriangulatedMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid DNA binary for testing.
    fn synthetic_dna() -> Vec<u8> {
        let mut buf = Vec::new();

        // Magic + version
        buf.extend_from_slice(b"DNA");
        buf.extend_from_slice(&2u16.to_be_bytes()); // generation
        buf.extend_from_slice(&1u16.to_be_bytes()); // version

        // Section offsets (filled later)
        let offsets_pos = buf.len();
        buf.extend_from_slice(&[0u8; 32]); // 8 × u32

        // ── Descriptor ──
        let desc_off = buf.len() as u32;
        // name
        let name = b"TestChar";
        buf.extend_from_slice(&(name.len() as u32).to_be_bytes());
        buf.extend_from_slice(name);
        buf.extend_from_slice(&0u16.to_be_bytes()); // archetype
        buf.extend_from_slice(&1u16.to_be_bytes()); // gender
        buf.extend_from_slice(&25u16.to_be_bytes()); // age
        buf.extend_from_slice(&0u32.to_be_bytes()); // metadata count
        buf.extend_from_slice(&0u16.to_be_bytes()); // translationUnit
        buf.extend_from_slice(&0u16.to_be_bytes()); // rotationUnit
        buf.extend_from_slice(
            &[0u16.to_be_bytes(), 1u16.to_be_bytes(), 2u16.to_be_bytes()].concat(),
        ); // coordSys
        buf.extend_from_slice(&2u16.to_be_bytes()); // lodCount
        buf.extend_from_slice(&7u16.to_be_bytes()); // maxLOD
        buf.extend_from_slice(&0u32.to_be_bytes()); // complexity (empty string)
        buf.extend_from_slice(&0u32.to_be_bytes()); // dbName (empty string)

        // ── Definition ──
        let def_off = buf.len() as u32;
        // 4 LOD mappings (empty)
        for _ in 0..4 {
            buf.extend_from_slice(&0u32.to_be_bytes()); // lods count
            buf.extend_from_slice(&0u32.to_be_bytes()); // indices outer count
        }
        // GUI controls (0)
        buf.extend_from_slice(&0u32.to_be_bytes());
        // Raw controls (0)
        buf.extend_from_slice(&0u32.to_be_bytes());
        // Joint names (2)
        buf.extend_from_slice(&2u32.to_be_bytes());
        for n in [b"root" as &[u8], b"head"] {
            buf.extend_from_slice(&(n.len() as u32).to_be_bytes());
            buf.extend_from_slice(n);
        }
        // Blend shape channels (1)
        buf.extend_from_slice(&1u32.to_be_bytes());
        let bs = b"jawOpen";
        buf.extend_from_slice(&(bs.len() as u32).to_be_bytes());
        buf.extend_from_slice(bs);
        // Animated maps (0)
        buf.extend_from_slice(&0u32.to_be_bytes());
        // Mesh names (1)
        buf.extend_from_slice(&1u32.to_be_bytes());
        let mn = b"head_lod0";
        buf.extend_from_slice(&(mn.len() as u32).to_be_bytes());
        buf.extend_from_slice(mn);
        // meshBlendShapeChannelMapping (from + to, both empty)
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        // Joint hierarchy
        buf.extend_from_slice(&2u32.to_be_bytes());
        buf.extend_from_slice(&0xFFFFu16.to_be_bytes()); // root: no parent
        buf.extend_from_slice(&0u16.to_be_bytes()); // head: parent=root
                                                    // Neutral joint translations (SoA)
        buf.extend_from_slice(&2u32.to_be_bytes()); // xs
        buf.extend_from_slice(&0.0f32.to_be_bytes());
        buf.extend_from_slice(&0.0f32.to_be_bytes());
        buf.extend_from_slice(&2u32.to_be_bytes()); // ys
        buf.extend_from_slice(&0.0f32.to_be_bytes());
        buf.extend_from_slice(&10.0f32.to_be_bytes()); // head at 10cm
        buf.extend_from_slice(&2u32.to_be_bytes()); // zs
        buf.extend_from_slice(&0.0f32.to_be_bytes());
        buf.extend_from_slice(&0.0f32.to_be_bytes());
        // Neutral joint rotations (SoA, degrees)
        for _ in 0..3 {
            buf.extend_from_slice(&2u32.to_be_bytes());
            buf.extend_from_slice(&0.0f32.to_be_bytes());
            buf.extend_from_slice(&0.0f32.to_be_bytes());
        }

        // ── Geometry ──
        let geom_off = buf.len() as u32;
        buf.extend_from_slice(&1u32.to_be_bytes()); // 1 mesh
        buf.extend_from_slice(&0u32.to_be_bytes()); // mesh end-offset (ArchiveOffset, ignored on read)

        // Positions (SoA) — 4 vertices (quad) in centimeters
        for vals in [
            [-5.0f32, 5.0, 5.0, -5.0], // xs
            [0.0, 0.0, 10.0, 10.0],    // ys
            [0.0, 0.0, 0.0, 0.0],      // zs
        ] {
            buf.extend_from_slice(&(vals.len() as u32).to_be_bytes());
            for v in vals {
                buf.extend_from_slice(&v.to_be_bytes());
            }
        }
        // UVs
        buf.extend_from_slice(&4u32.to_be_bytes()); // Us
        for v in [0.0f32, 1.0, 1.0, 0.0] {
            buf.extend_from_slice(&v.to_be_bytes());
        }
        buf.extend_from_slice(&4u32.to_be_bytes()); // Vs
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            buf.extend_from_slice(&v.to_be_bytes());
        }
        // Normals (SoA)
        for vals in [
            [0.0f32, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
        ] {
            buf.extend_from_slice(&(vals.len() as u32).to_be_bytes());
            for v in vals {
                buf.extend_from_slice(&v.to_be_bytes());
            }
        }
        // Vertex layouts (identity mapping)
        for _ in 0..3 {
            buf.extend_from_slice(&4u32.to_be_bytes());
            for i in 0..4u32 {
                buf.extend_from_slice(&i.to_be_bytes());
            }
        }
        // Faces: 1 quad face
        buf.extend_from_slice(&1u32.to_be_bytes()); // 1 face
        buf.extend_from_slice(&4u32.to_be_bytes()); // 4 vertices in face
        for i in 0..4u32 {
            buf.extend_from_slice(&i.to_be_bytes());
        }
        // Skin weights: 0
        buf.extend_from_slice(&0u16.to_be_bytes()); // max_influences
        buf.extend_from_slice(&0u32.to_be_bytes()); // count
                                                    // Blend shapes: 0
        buf.extend_from_slice(&0u32.to_be_bytes());

        // Backpatch section offsets
        let offsets = [desc_off, def_off, 0, 0, 0, 0, 0, geom_off];
        for (i, &off) in offsets.iter().enumerate() {
            let p = offsets_pos + i * 4;
            buf[p..p + 4].copy_from_slice(&off.to_be_bytes());
        }

        // EOF marker
        buf.extend_from_slice(b"AND");

        buf
    }

    #[test]
    fn test_parse_header() {
        let data = synthetic_dna();
        let dna = DnaFile::from_bytes(&data).unwrap();
        assert_eq!(dna.header.generation, 2);
        assert_eq!(dna.header.version, 1);
    }

    #[test]
    fn test_parse_descriptor() {
        let dna = DnaFile::from_bytes(&synthetic_dna()).unwrap();
        assert_eq!(dna.descriptor.name, "TestChar");
        assert_eq!(dna.descriptor.age, 25);
        assert_eq!(dna.descriptor.lod_count, 2);
    }

    #[test]
    fn test_parse_definition() {
        let dna = DnaFile::from_bytes(&synthetic_dna()).unwrap();
        assert_eq!(dna.definition.joint_names.len(), 2);
        assert_eq!(dna.definition.joint_names[0], "root");
        assert_eq!(dna.definition.joint_names[1], "head");
        assert_eq!(dna.definition.mesh_names[0], "head_lod0");
        assert_eq!(dna.definition.blend_shape_channel_names[0], "jawOpen");
    }

    #[test]
    fn test_parse_geometry() {
        let dna = DnaFile::from_bytes(&synthetic_dna()).unwrap();
        assert_eq!(dna.geometry.meshes.len(), 1);
        let mesh = &dna.geometry.meshes[0];
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.faces.len(), 1);
        assert_eq!(mesh.faces[0].len(), 4); // quad face
                                            // Positions are converted from cm to m
        assert!((mesh.positions.xs[0] - (-0.05)).abs() < 0.001); // -5cm → -0.05m
    }

    #[test]
    fn test_triangulate() {
        let dna = DnaFile::from_bytes(&synthetic_dna()).unwrap();
        let tri = dna.triangulate_mesh(0);
        // 1 quad → 2 triangles → 6 indices
        assert_eq!(tri.indices.len(), 6);
        assert_eq!(tri.positions.len(), 4);
    }

    #[test]
    fn test_to_skeleton() {
        let dna = DnaFile::from_bytes(&synthetic_dna()).unwrap();
        let skel = dna.to_skeleton();
        assert_eq!(skel.bones.len(), 2);
        assert!(skel.bones[0].parent.is_none()); // root
        assert_eq!(skel.bones[1].parent, Some(0)); // head → root
    }

    #[test]
    fn test_totals() {
        let dna = DnaFile::from_bytes(&synthetic_dna()).unwrap();
        assert_eq!(dna.total_vertices(), 4);
        assert_eq!(dna.total_faces(), 1);
    }
}
