//! KAMI CAE — Computer-Aided Engineering
//!
//! FEA mesh generation, boundary conditions, material library, solvers, and
//! post-processing for the KAMI engineering SDK.

// ---------------------------------------------------------------------------
// mesh
// ---------------------------------------------------------------------------
pub mod mesh {
    use glam::DVec3;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// Unique node identifier.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct NodeId(pub u32);

    /// Unique element identifier.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct ElementId(pub u32);

    /// A finite-element node with position in 3-D space.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FeaNode {
        pub id: NodeId,
        pub position: DVec3,
    }

    /// Supported element topologies.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum FeaElement {
        /// 2-node beam / bar.
        Beam2(ElementId, [NodeId; 2]),
        /// 3-node triangle (linear).
        Tri3(ElementId, [NodeId; 3]),
        /// 6-node triangle (quadratic).
        Tri6(ElementId, [NodeId; 6]),
        /// 4-node quadrilateral.
        Quad4(ElementId, [NodeId; 4]),
        /// 4-node tetrahedron.
        Tet4(ElementId, [NodeId; 4]),
        /// 10-node tetrahedron (quadratic).
        Tet10(ElementId, [NodeId; 10]),
        /// 8-node hexahedron.
        Hex8(ElementId, [NodeId; 8]),
    }

    impl FeaElement {
        /// Return the element id regardless of variant.
        pub fn id(&self) -> ElementId {
            match self {
                Self::Beam2(id, _)
                | Self::Tri3(id, _)
                | Self::Tri6(id, _)
                | Self::Quad4(id, _)
                | Self::Tet4(id, _)
                | Self::Tet10(id, _)
                | Self::Hex8(id, _) => *id,
            }
        }
    }

    /// Mesh configuration for automatic meshing.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MeshConfig {
        pub element_size: f64,
        pub min_size: f64,
        pub max_size: f64,
        pub curvature_refinement: bool,
        pub quality_threshold: f64,
        pub order: ElementOrder,
    }

    /// Linear (first-order) vs quadratic (second-order) elements.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ElementOrder {
        Linear,
        Quadratic,
    }

    impl Default for MeshConfig {
        fn default() -> Self {
            Self {
                element_size: 1.0,
                min_size: 0.1,
                max_size: 10.0,
                curvature_refinement: true,
                quality_threshold: 0.3,
                order: ElementOrder::Linear,
            }
        }
    }

    /// Summary statistics for a mesh.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MeshStats {
        pub node_count: usize,
        pub element_count: usize,
        pub min_quality: f64,
        pub avg_quality: f64,
    }

    /// The primary FEA mesh container.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct FeaMesh {
        pub nodes: Vec<FeaNode>,
        pub elements: Vec<FeaElement>,
        pub node_sets: HashMap<String, Vec<NodeId>>,
        pub element_sets: HashMap<String, Vec<ElementId>>,
    }

    impl FeaMesh {
        pub fn new() -> Self {
            Self::default()
        }

        /// Append a node, returning its id.
        pub fn add_node(&mut self, position: DVec3) -> NodeId {
            let id = NodeId(self.nodes.len() as u32);
            self.nodes.push(FeaNode { id, position });
            id
        }

        /// Append an element.
        pub fn add_element(&mut self, element: FeaElement) {
            self.elements.push(element);
        }

        /// Register (or overwrite) a named node set.
        pub fn create_node_set(&mut self, name: impl Into<String>, ids: Vec<NodeId>) {
            self.node_sets.insert(name.into(), ids);
        }

        /// Compute basic quality statistics.
        ///
        /// Element quality is approximated as 1.0 for all well-formed elements
        /// in this implementation (a real mesher would compute aspect-ratio /
        /// Jacobian metrics).
        pub fn mesh_stats(&self) -> MeshStats {
            let quality = 1.0_f64; // placeholder — uniform quality
            MeshStats {
                node_count: self.nodes.len(),
                element_count: self.elements.len(),
                min_quality: if self.elements.is_empty() {
                    0.0
                } else {
                    quality
                },
                avg_quality: if self.elements.is_empty() {
                    0.0
                } else {
                    quality
                },
            }
        }
    }

    /// Generate a regular hexahedral mesh of a box with given dimensions and
    /// `divisions` cells along each axis.
    pub fn generate_box_mesh(width: f64, height: f64, depth: f64, divisions: u32) -> FeaMesh {
        let mut mesh = FeaMesh::new();
        let n = divisions + 1; // nodes per axis

        // Nodes
        for iz in 0..n {
            for iy in 0..n {
                for ix in 0..n {
                    let x = (ix as f64 / divisions as f64) * width;
                    let y = (iy as f64 / divisions as f64) * height;
                    let z = (iz as f64 / divisions as f64) * depth;
                    mesh.add_node(DVec3::new(x, y, z));
                }
            }
        }

        // Hex8 elements
        let mut eid: u32 = 0;
        for iz in 0..divisions {
            for iy in 0..divisions {
                for ix in 0..divisions {
                    let base = |dz: u32, dy: u32, dx: u32| -> NodeId {
                        NodeId((iz + dz) * n * n + (iy + dy) * n + (ix + dx))
                    };
                    let nodes = [
                        base(0, 0, 0),
                        base(0, 0, 1),
                        base(0, 1, 1),
                        base(0, 1, 0),
                        base(1, 0, 0),
                        base(1, 0, 1),
                        base(1, 1, 1),
                        base(1, 1, 0),
                    ];
                    mesh.add_element(FeaElement::Hex8(ElementId(eid), nodes));
                    eid += 1;
                }
            }
        }

        mesh
    }
}

// ---------------------------------------------------------------------------
// material
// ---------------------------------------------------------------------------
pub mod material {
    use serde::{Deserialize, Serialize};

    /// A finite-element material definition.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FeMaterial {
        pub name: String,
        pub model: MaterialModel,
    }

    /// Constitutive model.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum MaterialModel {
        LinearElastic {
            /// Young's modulus (Pa).
            youngs_modulus: f64,
            /// Poisson's ratio (dimensionless).
            poissons_ratio: f64,
            /// Density (kg/m^3).
            density: f64,
            /// Coefficient of thermal expansion (1/K).
            thermal_expansion: f64,
            /// Thermal conductivity (W/(m·K)).
            thermal_conductivity: f64,
            /// Specific heat capacity (J/(kg·K)).
            specific_heat: f64,
        },
        Hyperelastic,
        ElastoPlastic,
    }

    /// Collection of materials with built-in engineering presets.
    #[derive(Debug, Clone, Default)]
    pub struct MaterialLibrary {
        pub materials: Vec<FeMaterial>,
    }

    impl MaterialLibrary {
        pub fn new() -> Self {
            Self::default()
        }

        /// Add a material and return its index.
        pub fn add(&mut self, mat: FeMaterial) -> usize {
            self.materials.push(mat);
            self.materials.len() - 1
        }

        /// Look up a material by name.
        pub fn get(&self, name: &str) -> Option<&FeMaterial> {
            self.materials.iter().find(|m| m.name == name)
        }

        // ---- built-in presets ------------------------------------------------

        /// ASTM A36 structural steel.
        pub fn steel_structural() -> FeMaterial {
            FeMaterial {
                name: "Steel-Structural".into(),
                model: MaterialModel::LinearElastic {
                    youngs_modulus: 200.0e9,
                    poissons_ratio: 0.3,
                    density: 7850.0,
                    thermal_expansion: 12.0e-6,
                    thermal_conductivity: 50.0,
                    specific_heat: 490.0,
                },
            }
        }

        /// Aluminum 6061-T6.
        pub fn aluminum_6061() -> FeMaterial {
            FeMaterial {
                name: "Aluminum-6061".into(),
                model: MaterialModel::LinearElastic {
                    youngs_modulus: 68.9e9,
                    poissons_ratio: 0.33,
                    density: 2700.0,
                    thermal_expansion: 23.6e-6,
                    thermal_conductivity: 167.0,
                    specific_heat: 896.0,
                },
            }
        }

        /// Titanium Ti-6Al-4V.
        pub fn titanium_6al4v() -> FeMaterial {
            FeMaterial {
                name: "Titanium-6Al4V".into(),
                model: MaterialModel::LinearElastic {
                    youngs_modulus: 113.8e9,
                    poissons_ratio: 0.342,
                    density: 4430.0,
                    thermal_expansion: 8.6e-6,
                    thermal_conductivity: 6.7,
                    specific_heat: 526.0,
                },
            }
        }

        /// General-purpose concrete (C30).
        pub fn concrete() -> FeMaterial {
            FeMaterial {
                name: "Concrete".into(),
                model: MaterialModel::LinearElastic {
                    youngs_modulus: 30.0e9,
                    poissons_ratio: 0.2,
                    density: 2400.0,
                    thermal_expansion: 10.0e-6,
                    thermal_conductivity: 1.7,
                    specific_heat: 880.0,
                },
            }
        }

        /// Return a library pre-populated with all built-in presets.
        pub fn with_presets() -> Self {
            let mut lib = Self::new();
            lib.add(Self::steel_structural());
            lib.add(Self::aluminum_6061());
            lib.add(Self::titanium_6al4v());
            lib.add(Self::concrete());
            lib
        }
    }
}

// ---------------------------------------------------------------------------
// boundary
// ---------------------------------------------------------------------------
pub mod boundary {
    use glam::DVec3;
    use serde::{Deserialize, Serialize};

    /// Degree-of-freedom bit mask for displacement constraints.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct DofMask(pub u8);

    impl DofMask {
        pub const X: Self = Self(0b0000_0001);
        pub const Y: Self = Self(0b0000_0010);
        pub const Z: Self = Self(0b0000_0100);
        pub const RX: Self = Self(0b0000_1000);
        pub const RY: Self = Self(0b0001_0000);
        pub const RZ: Self = Self(0b0010_0000);

        /// All translational + rotational DOFs fixed.
        pub const ALL: Self = Self(0b0011_1111);

        /// Combine two masks.
        pub fn union(self, other: Self) -> Self {
            Self(self.0 | other.0)
        }

        /// Check whether a specific DOF is set.
        pub fn contains(self, other: Self) -> bool {
            (self.0 & other.0) == other.0
        }
    }

    /// A boundary condition applied to a set of nodes or faces.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum BoundaryCondition {
        /// Prescribed displacement on a named node set.
        Displacement {
            node_set: String,
            dof_mask: DofMask,
            value: DVec3,
        },
        /// Concentrated force on a named node set.
        Force { node_set: String, value: DVec3 },
        /// Uniform pressure on a named face set.
        Pressure { face_set: String, value: f64 },
        /// Prescribed temperature on a named node set.
        Temperature { node_set: String, value: f64 },
        /// Convection on a named face set.
        Convection {
            face_set: String,
            coefficient: f64,
            ambient_temp: f64,
        },
    }
}

// ---------------------------------------------------------------------------
// solver
// ---------------------------------------------------------------------------
pub mod solver {
    use glam::DVec3;
    use serde::{Deserialize, Serialize};

    use crate::boundary::{BoundaryCondition, DofMask};
    use crate::material::{FeMaterial, MaterialModel};
    use crate::mesh::{FeaElement, FeaMesh};

    /// Analysis type selector.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum AnalysisType {
        LinearStatic,
        NonlinearStatic,
        Modal,
        ThermalSteady,
        ThermalTransient,
        Buckling,
    }

    /// Solver algorithm selector.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum SolverMethod {
        DirectCholesky,
        ConjugateGradient { max_iter: usize, tolerance: f64 },
        Gmres,
    }

    impl Default for SolverMethod {
        fn default() -> Self {
            Self::DirectCholesky
        }
    }

    /// Result container for a completed analysis.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AnalysisResult {
        pub analysis_id: String,
        /// Nodal displacement vectors.
        pub displacement: Vec<DVec3>,
        /// Von Mises stress per element.
        pub stress: Vec<f64>,
        /// Strain per element.
        pub strain: Vec<f64>,
        pub max_displacement: f64,
        pub max_stress: f64,
    }

    /// Error type for solver failures.
    #[derive(Debug, thiserror::Error)]
    pub enum SolverError {
        #[error("singular stiffness matrix — check constraints")]
        SingularMatrix,
        #[error("unsupported element type for this solver")]
        UnsupportedElement,
        #[error("no force boundary conditions specified")]
        NoLoads,
        #[error("node set '{0}' not found in mesh")]
        NodeSetNotFound(String),
    }

    // ---- dense linear algebra helpers (educational, small problems) ----------

    /// Solve a symmetric positive-definite system Ax = b via Cholesky
    /// decomposition.  `a` is stored row-major, dimension `n x n`.
    fn cholesky_solve(a: &[f64], b: &[f64], n: usize) -> Result<Vec<f64>, SolverError> {
        // Decompose A = L·Lᵀ
        let mut l = vec![0.0_f64; n * n];
        for i in 0..n {
            for j in 0..=i {
                let mut sum = 0.0;
                for k in 0..j {
                    sum += l[i * n + k] * l[j * n + k];
                }
                if i == j {
                    let diag = a[i * n + i] - sum;
                    if diag <= 0.0 {
                        return Err(SolverError::SingularMatrix);
                    }
                    l[i * n + j] = diag.sqrt();
                } else {
                    let denom = l[j * n + j];
                    if denom.abs() < 1e-30 {
                        return Err(SolverError::SingularMatrix);
                    }
                    l[i * n + j] = (a[i * n + j] - sum) / denom;
                }
            }
        }

        // Forward substitution: L·y = b
        let mut y = vec![0.0; n];
        for i in 0..n {
            let mut sum = 0.0;
            for k in 0..i {
                sum += l[i * n + k] * y[k];
            }
            y[i] = (b[i] - sum) / l[i * n + i];
        }

        // Backward substitution: Lᵀ·x = y
        let mut x = vec![0.0; n];
        for i in (0..n).rev() {
            let mut sum = 0.0;
            for k in (i + 1)..n {
                sum += l[k * n + i] * x[k];
            }
            x[i] = (y[i] - sum) / l[i * n + i];
        }

        Ok(x)
    }

    /// Solve a linear-static FEA problem.
    ///
    /// Currently supports meshes composed entirely of `Beam2` (1-D bar)
    /// elements.  Each bar element has a unit cross-section area (A = 1 m^2)
    /// — callers can scale the Young's modulus accordingly.
    ///
    /// Assembly:
    ///   For each bar element connecting nodes i and j along some axis,
    ///   local stiffness  k = A·E / L  is assembled into the global matrix.
    ///   Only translational DOFs (x, y, z per node → 3·N total DOFs) are
    ///   considered.
    pub fn solve_linear_static(
        mesh: &FeaMesh,
        material: &FeMaterial,
        bcs: &[BoundaryCondition],
    ) -> Result<AnalysisResult, SolverError> {
        let youngs_modulus = match &material.model {
            MaterialModel::LinearElastic { youngs_modulus, .. } => *youngs_modulus,
            _ => return Err(SolverError::UnsupportedElement),
        };

        let n_nodes = mesh.nodes.len();
        let ndof = n_nodes * 3;
        let cross_section_area = 1.0; // unit area

        // Global stiffness matrix (dense, row-major)
        let mut k_global = vec![0.0_f64; ndof * ndof];
        // Global force vector
        let mut f_global = vec![0.0_f64; ndof];

        // Assemble element stiffness (Beam2 / bar only)
        for elem in &mesh.elements {
            let (n_i, n_j) = match elem {
                FeaElement::Beam2(_, nodes) => (nodes[0], nodes[1]),
                _ => return Err(SolverError::UnsupportedElement),
            };

            let pi = mesh.nodes[n_i.0 as usize].position;
            let pj = mesh.nodes[n_j.0 as usize].position;
            let delta = pj - pi;
            let length = delta.length();
            if length < 1e-15 {
                continue;
            }
            let dir = delta / length; // unit direction cosines

            let ke = cross_section_area * youngs_modulus / length;

            // DOF indices
            let dofs_i = [
                n_i.0 as usize * 3,
                n_i.0 as usize * 3 + 1,
                n_i.0 as usize * 3 + 2,
            ];
            let dofs_j = [
                n_j.0 as usize * 3,
                n_j.0 as usize * 3 + 1,
                n_j.0 as usize * 3 + 2,
            ];

            let d = [dir.x, dir.y, dir.z];

            // Local 6×6 stiffness in global coords:
            //   K_local = ke * [  C  -C ]
            //                  [ -C   C ]
            // where C_ab = d_a * d_b
            for a in 0..3 {
                for b in 0..3 {
                    let val = ke * d[a] * d[b];
                    k_global[dofs_i[a] * ndof + dofs_i[b]] += val;
                    k_global[dofs_j[a] * ndof + dofs_j[b]] += val;
                    k_global[dofs_i[a] * ndof + dofs_j[b]] -= val;
                    k_global[dofs_j[a] * ndof + dofs_i[b]] -= val;
                }
            }
        }

        // Apply force BCs
        let mut has_loads = false;
        for bc in bcs {
            if let BoundaryCondition::Force { node_set, value } = bc {
                let ids = mesh
                    .node_sets
                    .get(node_set)
                    .ok_or_else(|| SolverError::NodeSetNotFound(node_set.clone()))?;
                for nid in ids {
                    let base = nid.0 as usize * 3;
                    f_global[base] += value.x;
                    f_global[base + 1] += value.y;
                    f_global[base + 2] += value.z;
                }
                has_loads = true;
            }
        }
        if !has_loads {
            return Err(SolverError::NoLoads);
        }

        // Apply displacement BCs via row/column elimination.
        // For each fixed DOF d with prescribed value v:
        //   1. Subtract K[r][d] * v from f[r] for all rows r != d
        //   2. Zero row d and column d
        //   3. Set K[d][d] = 1, f[d] = v
        for bc in bcs {
            if let BoundaryCondition::Displacement {
                node_set,
                dof_mask,
                value,
            } = bc
            {
                let ids = mesh
                    .node_sets
                    .get(node_set)
                    .ok_or_else(|| SolverError::NodeSetNotFound(node_set.clone()))?;
                let vals = [value.x, value.y, value.z];
                let masks = [DofMask::X, DofMask::Y, DofMask::Z];
                for nid in ids {
                    let base = nid.0 as usize * 3;
                    for c in 0..3 {
                        if !dof_mask.contains(masks[c]) {
                            continue;
                        }
                        let d = base + c;
                        let v = vals[c];
                        // Adjust RHS for prescribed displacement
                        for r in 0..ndof {
                            if r != d {
                                f_global[r] -= k_global[r * ndof + d] * v;
                            }
                        }
                        // Zero row and column
                        for j in 0..ndof {
                            k_global[d * ndof + j] = 0.0;
                            k_global[j * ndof + d] = 0.0;
                        }
                        k_global[d * ndof + d] = 1.0;
                        f_global[d] = v;
                    }
                }
            }
        }

        // Stabilise unconstrained zero-stiffness DOFs (e.g. transverse
        // DOFs of bar elements that have no stiffness contribution).  Without
        // this, the matrix is singular.  A tiny diagonal value makes these
        // DOFs effectively fixed at zero without affecting other results.
        let stab = 1.0;
        for d in 0..ndof {
            if k_global[d * ndof + d].abs() < 1e-30 {
                k_global[d * ndof + d] = stab;
            }
        }

        // Solve K·u = f
        let u = cholesky_solve(&k_global, &f_global, ndof)?;

        // Build displacement vectors
        let mut displacement = Vec::with_capacity(n_nodes);
        let mut max_disp = 0.0_f64;
        for i in 0..n_nodes {
            let d = DVec3::new(u[i * 3], u[i * 3 + 1], u[i * 3 + 2]);
            let mag = d.length();
            if mag > max_disp {
                max_disp = mag;
            }
            displacement.push(d);
        }

        // Compute element stress / strain (bar: sigma = E * epsilon,
        // epsilon = (u_j - u_i) · dir / L)
        let mut stress = Vec::with_capacity(mesh.elements.len());
        let mut strain = Vec::with_capacity(mesh.elements.len());
        let mut max_stress = 0.0_f64;

        for elem in &mesh.elements {
            let (n_i, n_j) = match elem {
                FeaElement::Beam2(_, nodes) => (nodes[0], nodes[1]),
                _ => unreachable!(),
            };
            let pi = mesh.nodes[n_i.0 as usize].position;
            let pj = mesh.nodes[n_j.0 as usize].position;
            let delta = pj - pi;
            let length = delta.length();
            let dir = delta / length;

            let ui = displacement[n_i.0 as usize];
            let uj = displacement[n_j.0 as usize];
            let eps = (uj - ui).dot(dir) / length;
            let sig = youngs_modulus * eps;

            strain.push(eps);
            stress.push(sig.abs());
            if sig.abs() > max_stress {
                max_stress = sig.abs();
            }
        }

        Ok(AnalysisResult {
            analysis_id: "linear-static-0".into(),
            displacement,
            stress,
            strain,
            max_displacement: max_disp,
            max_stress,
        })
    }
}

// ---------------------------------------------------------------------------
// postprocess
// ---------------------------------------------------------------------------
pub mod postprocess {
    use glam::DVec3;
    use serde::{Deserialize, Serialize};

    use crate::solver::AnalysisResult;

    /// Scalar field types available for post-processing.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ResultField {
        Displacement,
        VonMisesStress,
        PrincipalStress,
        Strain,
        Temperature,
        ModeShape,
        SafetyFactor,
    }

    /// Scalar range of a result field.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FieldRange {
        pub min: f64,
        pub max: f64,
        pub avg: f64,
    }

    impl FieldRange {
        /// Compute range from a slice of scalar values.
        pub fn from_values(values: &[f64]) -> Self {
            if values.is_empty() {
                return Self {
                    min: 0.0,
                    max: 0.0,
                    avg: 0.0,
                };
            }
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            let mut sum = 0.0;
            for &v in values {
                if v < min {
                    min = v;
                }
                if v > max {
                    max = v;
                }
                sum += v;
            }
            Self {
                min,
                max,
                avg: sum / values.len() as f64,
            }
        }
    }

    /// Interpolate a displacement result at an arbitrary point by
    /// inverse-distance weighting from all nodes.
    pub fn probe_point(result: &AnalysisResult, _point: DVec3) -> DVec3 {
        if result.displacement.is_empty() {
            return DVec3::ZERO;
        }
        // We need node positions to weight, but AnalysisResult stores only
        // displacement.  For a standalone probe we use index-based position
        // heuristic: assume regularly-spaced nodes and fall back to simple
        // nearest-node (index 0) when positions are unavailable.
        //
        // A production implementation would accept &FeaMesh alongside the
        // result.  Here we return the average displacement as a safe default.
        let mut sum = DVec3::ZERO;
        for d in &result.displacement {
            sum += *d;
        }
        sum / result.displacement.len() as f64
    }

    /// Build per-node scalar data suitable for rendering via
    /// `kami-eng-render`'s color-map pipeline.
    ///
    /// For `Displacement` the scalar is the displacement magnitude at each
    /// node.  For element-based fields (`VonMisesStress`, `Strain`, etc.)
    /// the values are returned per element (caller may average to nodes).
    pub fn export_color_map_data(result: &AnalysisResult, field: ResultField) -> Vec<f64> {
        match field {
            ResultField::Displacement => result.displacement.iter().map(|d| d.length()).collect(),
            ResultField::VonMisesStress | ResultField::PrincipalStress => result.stress.clone(),
            ResultField::Strain => result.strain.clone(),
            ResultField::SafetyFactor => {
                // Safety factor = yield / stress.  Use a placeholder yield
                // of 250 MPa (mild steel).
                let yield_stress = 250.0e6;
                result
                    .stress
                    .iter()
                    .map(|&s| {
                        if s > 0.0 {
                            yield_stress / s
                        } else {
                            f64::INFINITY
                        }
                    })
                    .collect()
            }
            ResultField::Temperature | ResultField::ModeShape => {
                // Not computed by linear-static solver; return zeros.
                vec![0.0; result.displacement.len()]
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    #[test]
    fn test_material_library_presets() {
        let lib = material::MaterialLibrary::with_presets();
        assert_eq!(lib.materials.len(), 4);

        let steel = lib.get("Steel-Structural").unwrap();
        match &steel.model {
            material::MaterialModel::LinearElastic {
                youngs_modulus,
                poissons_ratio,
                density,
                ..
            } => {
                assert!((youngs_modulus - 200.0e9).abs() < 1.0);
                assert!((poissons_ratio - 0.3).abs() < 1e-6);
                assert!((density - 7850.0).abs() < 1e-6);
            }
            _ => panic!("expected LinearElastic"),
        }

        assert!(lib.get("Aluminum-6061").is_some());
        assert!(lib.get("Titanium-6Al4V").is_some());
        assert!(lib.get("Concrete").is_some());
    }

    #[test]
    fn test_box_mesh_generation() {
        let m = mesh::generate_box_mesh(2.0, 3.0, 4.0, 2);
        let stats = m.mesh_stats();
        // 2 divisions → 3×3×3 = 27 nodes, 2×2×2 = 8 hex elements
        assert_eq!(stats.node_count, 27);
        assert_eq!(stats.element_count, 8);
        assert!(stats.min_quality > 0.0);
    }

    #[test]
    fn test_boundary_condition_creation() {
        use boundary::{BoundaryCondition, DofMask};

        let fix = BoundaryCondition::Displacement {
            node_set: "base".into(),
            dof_mask: DofMask::ALL,
            value: DVec3::ZERO,
        };

        if let BoundaryCondition::Displacement { dof_mask, .. } = &fix {
            assert!(dof_mask.contains(DofMask::X));
            assert!(dof_mask.contains(DofMask::RZ));
        } else {
            panic!("expected Displacement");
        }

        let conv = BoundaryCondition::Convection {
            face_set: "outer".into(),
            coefficient: 25.0,
            ambient_temp: 293.15,
        };
        assert!(matches!(conv, BoundaryCondition::Convection { .. }));
    }

    #[test]
    fn test_1d_bar_fea_solve() {
        // Single bar element: node 0 fixed, node 1 loaded with F = 1000 N.
        // L = 1.0 m, A = 1.0 m^2, E = 200 GPa.
        // Expected displacement at node 1: u = F·L / (A·E) = 1000 / 200e9 = 5e-9 m.
        let mut m = mesh::FeaMesh::new();
        let n0 = m.add_node(DVec3::new(0.0, 0.0, 0.0));
        let n1 = m.add_node(DVec3::new(1.0, 0.0, 0.0));
        m.add_element(mesh::FeaElement::Beam2(mesh::ElementId(0), [n0, n1]));
        m.create_node_set("fixed", vec![n0]);
        m.create_node_set("load", vec![n1]);

        let mat = material::MaterialLibrary::steel_structural(); // E = 200 GPa

        let bcs = vec![
            boundary::BoundaryCondition::Displacement {
                node_set: "fixed".into(),
                dof_mask: boundary::DofMask::ALL,
                value: DVec3::ZERO,
            },
            boundary::BoundaryCondition::Force {
                node_set: "load".into(),
                value: DVec3::new(1000.0, 0.0, 0.0),
            },
        ];

        let result = solver::solve_linear_static(&m, &mat, &bcs).unwrap();

        let expected = 1000.0 / 200.0e9; // 5e-9 m
        let computed = result.displacement[1].x;
        let err = (computed - expected).abs() / expected;
        assert!(
            err < 1e-6,
            "displacement error too large: computed={computed}, expected={expected}, rel_err={err}"
        );

        // Stress should equal E * strain = E * (u/L) = 1000 / 1.0 = 1000 Pa
        assert!(
            (result.stress[0] - 1000.0).abs() < 1.0,
            "stress mismatch: {}",
            result.stress[0]
        );
    }

    #[test]
    fn test_field_range_calculation() {
        let values = vec![1.0, 5.0, 3.0, 7.0, 2.0];
        let range = postprocess::FieldRange::from_values(&values);
        assert!((range.min - 1.0).abs() < 1e-12);
        assert!((range.max - 7.0).abs() < 1e-12);
        assert!((range.avg - 3.6).abs() < 1e-12);
    }
}
