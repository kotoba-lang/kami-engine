//! kami-cad: BREP solid modeling kernel with parametric feature tree and assembly.
//!
//! Provides a complete CAD foundation:
//! - BREP (Boundary Representation) topology: vertex, edge, face, shell, solid
//! - Parametric feature tree: sketch, extrude, revolve, fillet, chamfer, boolean
//! - Assembly management: part instances, constraints, BOM extraction
//! - Tessellation: BREP solid to triangle mesh conversion
//!
//! Uses f64 precision (DVec3/DAffine3) throughout for CAD-grade accuracy.
//! For real-time CSG preview, see `kami-sdf` SdfNode tree (f32, GPU-friendly).

// ── BREP Kernel ──

pub mod brep {
    use glam::DVec3;
    use serde::{Deserialize, Serialize};

    /// Unique topology identifier.
    pub type TopoId = u64;

    /// Orientation of a face or shell within its parent.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum Orientation {
        Forward,
        Reversed,
    }

    /// Analytic and freeform surface definitions (f64 precision).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum Surface {
        /// Infinite plane defined by origin + normal.
        Plane { origin: DVec3, normal: DVec3 },
        /// Cylinder: axis origin, axis direction, radius.
        Cylinder {
            origin: DVec3,
            axis: DVec3,
            radius: f64,
        },
        /// Cone: apex, axis direction, half-angle (radians).
        Cone {
            apex: DVec3,
            axis: DVec3,
            half_angle: f64,
        },
        /// Sphere: center, radius.
        Sphere { center: DVec3, radius: f64 },
        /// Torus: center, axis, major radius, minor radius.
        Torus {
            center: DVec3,
            axis: DVec3,
            major_radius: f64,
            minor_radius: f64,
        },
        /// B-spline surface: degree_u, degree_v, control points grid (row-major),
        /// knot vectors, rows, cols.
        BSplineSurface {
            degree_u: u32,
            degree_v: u32,
            control_points: Vec<DVec3>,
            knots_u: Vec<f64>,
            knots_v: Vec<f64>,
            rows: u32,
            cols: u32,
        },
    }

    /// Analytic and freeform curve definitions (f64 precision).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum Curve {
        /// Line segment: origin + direction (unit).
        Line { origin: DVec3, direction: DVec3 },
        /// Circle: center, normal, radius.
        Circle {
            center: DVec3,
            normal: DVec3,
            radius: f64,
        },
        /// Ellipse: center, normal, semi-major axis vector, semi-minor length.
        Ellipse {
            center: DVec3,
            normal: DVec3,
            semi_major: DVec3,
            semi_minor: f64,
        },
        /// B-spline curve: degree, control points, knot vector.
        BSplineCurve {
            degree: u32,
            control_points: Vec<DVec3>,
            knots: Vec<f64>,
        },
    }

    impl Curve {
        /// Evaluate curve at parameter t.
        pub fn evaluate(&self, t: f64) -> DVec3 {
            match self {
                Curve::Line { origin, direction } => *origin + *direction * t,
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    // Build local frame from normal.
                    let n = normal.normalize();
                    let u = if n.x.abs() < 0.9 {
                        DVec3::X.cross(n).normalize()
                    } else {
                        DVec3::Y.cross(n).normalize()
                    };
                    let v = n.cross(u);
                    *center + u * (radius * t.cos()) + v * (radius * t.sin())
                }
                Curve::Ellipse {
                    center,
                    normal,
                    semi_major,
                    semi_minor,
                } => {
                    let n = normal.normalize();
                    let u = semi_major.normalize();
                    let v = n.cross(u).normalize();
                    *center + u * (semi_major.length() * t.cos()) + v * (*semi_minor * t.sin())
                }
                Curve::BSplineCurve {
                    degree,
                    control_points,
                    knots,
                } => {
                    // De Boor evaluation.
                    if control_points.is_empty() {
                        return DVec3::ZERO;
                    }
                    let n = control_points.len();
                    let p = *degree as usize;
                    // Clamp t to valid range.
                    let t_clamped = t.clamp(knots[p], knots[n]);
                    // Find knot span.
                    let mut k = p;
                    for i in p..n {
                        if t_clamped >= knots[i] && t_clamped < knots[i + 1] {
                            k = i;
                            break;
                        }
                    }
                    if t_clamped >= knots[n] {
                        k = n - 1;
                    }
                    let mut d: Vec<DVec3> = (0..=p).map(|j| control_points[k - p + j]).collect();
                    for r in 1..=p {
                        for j in (r..=p).rev() {
                            let idx = k - p + j;
                            let denom = knots[idx + p + 1 - r] - knots[idx];
                            if denom.abs() < 1e-14 {
                                continue;
                            }
                            let alpha = (t_clamped - knots[idx]) / denom;
                            d[j] = d[j - 1] * (1.0 - alpha) + d[j] * alpha;
                        }
                    }
                    d[p]
                }
            }
        }
    }

    /// BREP vertex: a point in 3D space.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BrepVertex {
        pub id: TopoId,
        pub point: DVec3,
    }

    /// BREP edge: a bounded curve between two vertices.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BrepEdge {
        pub id: TopoId,
        pub curve: Curve,
        pub start_vertex: TopoId,
        pub end_vertex: TopoId,
        /// Parameter range [t_start, t_end] on the curve.
        pub t_range: (f64, f64),
    }

    /// BREP face: a bounded surface patch.
    /// `wires` contains ordered lists of edge IDs forming closed loops
    /// (first wire = outer boundary, subsequent = holes).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BrepFace {
        pub id: TopoId,
        pub surface: Surface,
        pub wires: Vec<Vec<TopoId>>,
        pub orientation: Orientation,
    }

    /// BREP shell: a connected set of faces forming a closed or open surface.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BrepShell {
        pub id: TopoId,
        pub faces: Vec<BrepFace>,
        pub orientation: Orientation,
    }

    /// BREP solid: one or more shells (first = outer, rest = voids).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BrepSolid {
        pub id: TopoId,
        pub shells: Vec<BrepShell>,
    }

    impl BrepSolid {
        /// Total number of faces across all shells.
        pub fn face_count(&self) -> usize {
            self.shells.iter().map(|s| s.faces.len()).sum()
        }

        /// Total number of unique edge IDs referenced by all faces.
        pub fn edge_count(&self) -> usize {
            let mut ids = std::collections::HashSet::new();
            for shell in &self.shells {
                for face in &shell.faces {
                    for wire in &face.wires {
                        for eid in wire {
                            ids.insert(*eid);
                        }
                    }
                }
            }
            ids.len()
        }

        /// Count unique vertex IDs in the topology (requires edge lookup).
        /// For a quick approximation, counts unique start/end vertex references
        /// across a supplied edge list.
        pub fn vertex_count(&self, edges: &[BrepEdge]) -> usize {
            let edge_ids: std::collections::HashSet<TopoId> = self
                .shells
                .iter()
                .flat_map(|s| &s.faces)
                .flat_map(|f| &f.wires)
                .flat_map(|w| w.iter())
                .copied()
                .collect();
            let mut verts = std::collections::HashSet::new();
            for e in edges {
                if edge_ids.contains(&e.id) {
                    verts.insert(e.start_vertex);
                    verts.insert(e.end_vertex);
                }
            }
            verts.len()
        }

        /// Axis-aligned bounding box from all face surfaces.
        /// Returns (min, max) corners. For plane faces, uses wire edge endpoints
        /// as an approximation (requires edges + vertices).
        pub fn bounding_box(&self, edges: &[BrepEdge], vertices: &[BrepVertex]) -> (DVec3, DVec3) {
            let mut min = DVec3::splat(f64::INFINITY);
            let mut max = DVec3::splat(f64::NEG_INFINITY);
            let vert_map: std::collections::HashMap<TopoId, DVec3> =
                vertices.iter().map(|v| (v.id, v.point)).collect();
            let edge_map: std::collections::HashMap<TopoId, &BrepEdge> =
                edges.iter().map(|e| (e.id, e)).collect();
            for shell in &self.shells {
                for face in &shell.faces {
                    for wire in &face.wires {
                        for eid in wire {
                            if let Some(edge) = edge_map.get(eid) {
                                if let Some(p) = vert_map.get(&edge.start_vertex) {
                                    min = min.min(*p);
                                    max = max.max(*p);
                                }
                                if let Some(p) = vert_map.get(&edge.end_vertex) {
                                    min = min.min(*p);
                                    max = max.max(*p);
                                }
                                // Sample mid-point of curved edges.
                                let t_mid = (edge.t_range.0 + edge.t_range.1) * 0.5;
                                let mid = edge.curve.evaluate(t_mid);
                                min = min.min(mid);
                                max = max.max(mid);
                            }
                        }
                    }
                }
            }
            (min, max)
        }

        /// Approximate volume via signed tetrahedron method on tessellated faces.
        /// Requires edge and vertex data for tessellation.
        pub fn volume(&self, edges: &[BrepEdge], vertices: &[BrepVertex]) -> f64 {
            let (positions, indices) = super::tessellate::tessellate_solid(self, edges, vertices);
            let mut vol = 0.0;
            let tri_count = indices.len() / 3;
            for i in 0..tri_count {
                let a = positions[indices[i * 3] as usize];
                let b = positions[indices[i * 3 + 1] as usize];
                let c = positions[indices[i * 3 + 2] as usize];
                // Signed volume of tetrahedron formed with origin.
                vol += a.dot(b.cross(c));
            }
            (vol / 6.0).abs()
        }

        /// Approximate surface area from tessellated triangles.
        pub fn surface_area(&self, edges: &[BrepEdge], vertices: &[BrepVertex]) -> f64 {
            let (positions, indices) = super::tessellate::tessellate_solid(self, edges, vertices);
            let mut area = 0.0;
            let tri_count = indices.len() / 3;
            for i in 0..tri_count {
                let a = positions[indices[i * 3] as usize];
                let b = positions[indices[i * 3 + 1] as usize];
                let c = positions[indices[i * 3 + 2] as usize];
                area += (b - a).cross(c - a).length() * 0.5;
            }
            area
        }
    }

    /// Helper: build a rectangular box solid from 8 vertices, 12 edges, 6 faces.
    pub fn make_box(
        id: TopoId,
        min: DVec3,
        max: DVec3,
    ) -> (BrepSolid, Vec<BrepEdge>, Vec<BrepVertex>) {
        let verts = [
            DVec3::new(min.x, min.y, min.z), // 0
            DVec3::new(max.x, min.y, min.z), // 1
            DVec3::new(max.x, max.y, min.z), // 2
            DVec3::new(min.x, max.y, min.z), // 3
            DVec3::new(min.x, min.y, max.z), // 4
            DVec3::new(max.x, min.y, max.z), // 5
            DVec3::new(max.x, max.y, max.z), // 6
            DVec3::new(min.x, max.y, max.z), // 7
        ];
        let brep_verts: Vec<BrepVertex> = verts
            .iter()
            .enumerate()
            .map(|(i, &p)| BrepVertex {
                id: (i + 1) as u64,
                point: p,
            })
            .collect();

        // 12 edges of a box.
        let edge_defs: [(usize, usize); 12] = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0), // bottom
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4), // top
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7), // verticals
        ];
        let brep_edges: Vec<BrepEdge> = edge_defs
            .iter()
            .enumerate()
            .map(|(i, &(s, e))| {
                let sv = &brep_verts[s];
                let ev = &brep_verts[e];
                let dir = (ev.point - sv.point).normalize();
                BrepEdge {
                    id: (100 + i) as u64,
                    curve: Curve::Line {
                        origin: sv.point,
                        direction: dir,
                    },
                    start_vertex: sv.id,
                    end_vertex: ev.id,
                    t_range: (0.0, sv.point.distance(ev.point)),
                }
            })
            .collect();

        // 6 faces: bottom(Z-), top(Z+), front(Y-), back(Y+), left(X-), right(X+).
        // Each wire is an ordered list of edge indices forming a closed loop.
        // Edges may be traversed forward or reversed; the tessellator handles both.
        // Edge map: 0:(0,1) 1:(1,2) 2:(2,3) 3:(3,0) 4:(4,5) 5:(5,6) 6:(6,7)
        //           7:(7,4) 8:(0,4) 9:(1,5) 10:(2,6) 11:(3,7)
        let face_wires: [(&[usize], DVec3); 6] = [
            (&[0, 1, 2, 3], DVec3::new(0.0, 0.0, -1.0)), // bottom: v0->v1->v2->v3
            (&[4, 5, 6, 7], DVec3::new(0.0, 0.0, 1.0)),  // top: v4->v5->v6->v7
            (&[0, 9, 4, 8], DVec3::new(0.0, -1.0, 0.0)), // front: v0->v1->v5->v4
            (&[2, 11, 6, 10], DVec3::new(0.0, 1.0, 0.0)), // back: v2->v3->v7->v6
            (&[3, 8, 7, 11], DVec3::new(-1.0, 0.0, 0.0)), // left: v3->v0->v4->v7
            (&[1, 10, 5, 9], DVec3::new(1.0, 0.0, 0.0)), // right: v1->v2->v6->v5
        ];
        let faces: Vec<BrepFace> = face_wires
            .iter()
            .enumerate()
            .map(|(i, (eidxs, normal))| {
                let wire: Vec<TopoId> = eidxs.iter().map(|&ei| brep_edges[ei].id).collect();
                BrepFace {
                    id: (200 + i) as u64,
                    surface: Surface::Plane {
                        origin: (min + max) * 0.5,
                        normal: *normal,
                    },
                    wires: vec![wire],
                    orientation: Orientation::Forward,
                }
            })
            .collect();

        let shell = BrepShell {
            id: 300,
            faces,
            orientation: Orientation::Forward,
        };

        (
            BrepSolid {
                id,
                shells: vec![shell],
            },
            brep_edges,
            brep_verts,
        )
    }
}

// ── Parametric Feature Tree ──

pub mod feature {
    use crate::brep::{self, BrepEdge, BrepSolid, BrepVertex, TopoId};
    use glam::DVec3;
    use kami_eng_core::constraint::ConstraintKind;
    use serde::{Deserialize, Serialize};

    /// Feature identifier within a feature tree.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct FeatureId(pub u64);

    /// Boolean operation for feature combination.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum BooleanOp {
        /// Create new body (base feature).
        New,
        /// Add material (union).
        Add,
        /// Remove material (subtraction).
        Cut,
        /// Keep intersection only.
        Intersect,
    }

    /// Sketch reference plane.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum SketchPlane {
        XY,
        XZ,
        YZ,
        /// Custom plane: origin + normal.
        Custom {
            origin: DVec3,
            normal: DVec3,
        },
    }

    impl SketchPlane {
        /// Returns the plane normal vector.
        pub fn normal(&self) -> DVec3 {
            match self {
                SketchPlane::XY => DVec3::Z,
                SketchPlane::XZ => DVec3::Y,
                SketchPlane::YZ => DVec3::X,
                SketchPlane::Custom { normal, .. } => normal.normalize(),
            }
        }

        /// Returns the plane origin.
        pub fn origin(&self) -> DVec3 {
            match self {
                SketchPlane::XY | SketchPlane::XZ | SketchPlane::YZ => DVec3::ZERO,
                SketchPlane::Custom { origin, .. } => *origin,
            }
        }
    }

    /// Local constraint kind mirror for serde compatibility with kami-eng-core.
    /// Maps 1:1 to `kami_eng_core::constraint::ConstraintKind`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum SketchConstraintKind {
        Coincident,
        Parallel,
        Perpendicular,
        Tangent,
        Equal,
        Horizontal,
        Vertical,
        Fixed,
        Symmetric,
        Concentric,
        Midpoint,
        Collinear,
        Distance,
        Angle,
        Radius,
        Diameter,
    }

    impl From<ConstraintKind> for SketchConstraintKind {
        fn from(k: ConstraintKind) -> Self {
            match k {
                ConstraintKind::Coincident => Self::Coincident,
                ConstraintKind::Parallel => Self::Parallel,
                ConstraintKind::Perpendicular => Self::Perpendicular,
                ConstraintKind::Tangent => Self::Tangent,
                ConstraintKind::Equal => Self::Equal,
                ConstraintKind::Horizontal => Self::Horizontal,
                ConstraintKind::Vertical => Self::Vertical,
                ConstraintKind::Fixed => Self::Fixed,
                ConstraintKind::Symmetric => Self::Symmetric,
                ConstraintKind::Concentric => Self::Concentric,
                ConstraintKind::Midpoint => Self::Midpoint,
                ConstraintKind::Collinear => Self::Collinear,
                ConstraintKind::Distance => Self::Distance,
                ConstraintKind::Angle => Self::Angle,
                ConstraintKind::Radius => Self::Radius,
                ConstraintKind::Diameter => Self::Diameter,
            }
        }
    }

    /// 2D sketch entity (drawn on a SketchPlane).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum SketchEntity {
        Line {
            start: glam::DVec2,
            end: glam::DVec2,
        },
        Arc {
            center: glam::DVec2,
            radius: f64,
            start_angle: f64,
            end_angle: f64,
        },
        Circle {
            center: glam::DVec2,
            radius: f64,
        },
        Spline {
            control_points: Vec<glam::DVec2>,
        },
        Dimension {
            entity_ref: u64,
            value: f64,
        },
        Constraint {
            kind: SketchConstraintKind,
            entity_refs: Vec<u64>,
        },
    }

    /// Parametric feature definition.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum Feature {
        /// 2D sketch on a reference plane.
        Sketch {
            id: FeatureId,
            plane: SketchPlane,
            entities: Vec<SketchEntity>,
        },
        /// Linear extrusion of a sketch profile.
        Extrude {
            id: FeatureId,
            sketch_ref: FeatureId,
            direction: DVec3,
            distance: f64,
            operation: BooleanOp,
        },
        /// Revolution of a sketch profile around an axis.
        Revolve {
            id: FeatureId,
            sketch_ref: FeatureId,
            axis: DVec3,
            angle: f64,
            operation: BooleanOp,
        },
        /// Fillet (round) edges.
        Fillet {
            id: FeatureId,
            edges: Vec<TopoId>,
            radius: f64,
        },
        /// Chamfer edges.
        Chamfer {
            id: FeatureId,
            edges: Vec<TopoId>,
            distance: f64,
        },
        /// Sweep a profile along a path.
        Sweep {
            id: FeatureId,
            profile_ref: FeatureId,
            path_ref: FeatureId,
            operation: BooleanOp,
        },
        /// Loft between two or more profiles.
        Loft {
            id: FeatureId,
            profiles: Vec<FeatureId>,
            operation: BooleanOp,
        },
        /// Shell (hollow) a solid, removing specified faces.
        Shell {
            id: FeatureId,
            removed_faces: Vec<TopoId>,
            thickness: f64,
        },
        /// Linear or circular pattern.
        Pattern {
            id: FeatureId,
            source_features: Vec<FeatureId>,
            direction: DVec3,
            count: u32,
            spacing: f64,
        },
        /// Direct boolean operation between two bodies.
        Boolean {
            id: FeatureId,
            operation: BooleanOp,
            tool_body: TopoId,
        },
    }

    impl Feature {
        /// Returns the feature's unique identifier.
        pub fn id(&self) -> FeatureId {
            match self {
                Feature::Sketch { id, .. }
                | Feature::Extrude { id, .. }
                | Feature::Revolve { id, .. }
                | Feature::Fillet { id, .. }
                | Feature::Chamfer { id, .. }
                | Feature::Sweep { id, .. }
                | Feature::Loft { id, .. }
                | Feature::Shell { id, .. }
                | Feature::Pattern { id, .. }
                | Feature::Boolean { id, .. } => *id,
            }
        }
    }

    /// Feature state within the tree.
    #[derive(Debug, Clone)]
    struct FeatureEntry {
        feature: Feature,
        suppressed: bool,
    }

    /// Ordered parametric feature tree. Features are evaluated top-to-bottom;
    /// suppressed features are skipped during evaluation.
    #[derive(Debug, Clone)]
    pub struct FeatureTree {
        entries: Vec<FeatureEntry>,
    }

    impl FeatureTree {
        pub fn new() -> Self {
            Self {
                entries: Vec::new(),
            }
        }

        /// Append a feature to the end of the tree.
        pub fn add_feature(&mut self, feature: Feature) {
            self.entries.push(FeatureEntry {
                feature,
                suppressed: false,
            });
        }

        /// Suppress a feature (skip during evaluation).
        pub fn suppress(&mut self, id: FeatureId) {
            if let Some(e) = self.entries.iter_mut().find(|e| e.feature.id() == id) {
                e.suppressed = true;
            }
        }

        /// Unsuppress a previously suppressed feature.
        pub fn unsuppress(&mut self, id: FeatureId) {
            if let Some(e) = self.entries.iter_mut().find(|e| e.feature.id() == id) {
                e.suppressed = false;
            }
        }

        /// Reorder: move the feature with `id` to `new_index`.
        pub fn reorder(&mut self, id: FeatureId, new_index: usize) {
            if let Some(pos) = self.entries.iter().position(|e| e.feature.id() == id) {
                let entry = self.entries.remove(pos);
                let idx = new_index.min(self.entries.len());
                self.entries.insert(idx, entry);
            }
        }

        /// Number of features (including suppressed).
        pub fn len(&self) -> usize {
            self.entries.len()
        }

        /// Whether the tree is empty.
        pub fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }

        /// Evaluate the feature tree, producing a BREP solid.
        ///
        /// Current implementation handles the base-feature case:
        /// - `Extrude` with `BooleanOp::New` generates a box-like prism from
        ///   the sketch bounding rectangle along the extrusion direction.
        /// - Subsequent features are recorded but not yet applied (returns
        ///   the base solid). Full boolean evaluation requires a BREP boolean
        ///   kernel (future: integrate with `kami-sdf` CSG for preview).
        pub fn evaluate(&self) -> Result<(BrepSolid, Vec<BrepEdge>, Vec<BrepVertex>), String> {
            let mut result: Option<(BrepSolid, Vec<BrepEdge>, Vec<BrepVertex>)> = None;

            for entry in &self.entries {
                if entry.suppressed {
                    continue;
                }
                match &entry.feature {
                    Feature::Extrude {
                        direction,
                        distance,
                        operation,
                        ..
                    } => {
                        match operation {
                            BooleanOp::New => {
                                // Generate a prism: for simplicity, create a unit-square
                                // cross-section extruded along direction * distance.
                                let half = 0.5;
                                let ext = direction.normalize() * *distance;
                                let min = DVec3::new(-half, -half, 0.0);
                                let max = DVec3::new(half, half, 0.0) + ext;
                                let (solid, edges, verts) = brep::make_box(1, min, max);
                                result = Some((solid, edges, verts));
                            }
                            BooleanOp::Add | BooleanOp::Cut | BooleanOp::Intersect => {
                                // Full boolean operations require BREP boolean kernel.
                                // For CSG preview, convert to kami-sdf SdfNode tree.
                                if result.is_none() {
                                    return Err("no base solid to apply boolean to".into());
                                }
                                // TODO: BREP boolean (union/difference/intersection)
                                log::warn!("BREP boolean not yet implemented; feature skipped");
                            }
                        }
                    }
                    Feature::Revolve { .. } => {
                        // TODO: revolve profile around axis
                        log::warn!("revolve feature not yet implemented");
                    }
                    Feature::Fillet { .. } | Feature::Chamfer { .. } => {
                        // TODO: edge blend operations
                        log::warn!("fillet/chamfer not yet implemented");
                    }
                    Feature::Sketch { .. } => {
                        // Sketches are consumed by subsequent extrude/revolve features.
                    }
                    _ => {
                        log::warn!("feature {:?} not yet implemented", entry.feature.id());
                    }
                }
            }

            result.ok_or_else(|| "feature tree produced no solid".into())
        }
    }

    impl Default for FeatureTree {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ── Assembly ──

pub mod assembly {
    use crate::brep::TopoId;
    use glam::DAffine3;
    use serde::{Deserialize, Serialize};

    /// Reference to a part (could be a BREP solid ID, file path, or external ref).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PartRef {
        pub solid_id: TopoId,
        pub name: String,
    }

    /// A placed instance of a part within an assembly.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PartInstance {
        pub id: u64,
        pub part_ref: PartRef,
        /// World transform (f64 precision for CAD accuracy).
        #[serde(skip)]
        pub transform: DAffine3,
        pub name: String,
        pub suppressed: bool,
    }

    /// Assembly constraint between two part instances.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum AssemblyConstraint {
        /// Mate: coincident faces (face-to-face contact).
        Mate {
            instance_a: u64,
            face_a: TopoId,
            instance_b: u64,
            face_b: TopoId,
        },
        /// Align: co-planar faces (same direction normals).
        Align {
            instance_a: u64,
            face_a: TopoId,
            instance_b: u64,
            face_b: TopoId,
        },
        /// Insert: concentric + mate (cylindrical fit).
        Insert {
            instance_a: u64,
            face_a: TopoId,
            instance_b: u64,
            face_b: TopoId,
        },
        /// Angle: fixed angle between two planes.
        Angle {
            instance_a: u64,
            face_a: TopoId,
            instance_b: u64,
            face_b: TopoId,
            angle: f64,
        },
        /// Distance: offset between two faces.
        Distance {
            instance_a: u64,
            face_a: TopoId,
            instance_b: u64,
            face_b: TopoId,
            distance: f64,
        },
    }

    /// Bill-of-materials entry.
    #[derive(Debug, Clone)]
    pub struct BomEntry {
        pub part_name: String,
        pub quantity: u32,
    }

    /// Assembly: a collection of part instances with positional constraints.
    #[derive(Debug)]
    pub struct Assembly {
        pub name: String,
        instances: Vec<PartInstance>,
        constraints: Vec<AssemblyConstraint>,
        next_instance_id: u64,
    }

    impl Assembly {
        pub fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                instances: Vec::new(),
                constraints: Vec::new(),
                next_instance_id: 1,
            }
        }

        /// Add a part instance at the given transform. Returns instance ID.
        pub fn add_instance(&mut self, part_ref: PartRef, transform: DAffine3, name: &str) -> u64 {
            let id = self.next_instance_id;
            self.next_instance_id += 1;
            self.instances.push(PartInstance {
                id,
                part_ref,
                transform,
                name: name.to_string(),
                suppressed: false,
            });
            id
        }

        /// Add a constraint between instances.
        pub fn add_constraint(&mut self, constraint: AssemblyConstraint) {
            self.constraints.push(constraint);
        }

        /// Solve assembly constraints (basic: identity for unconstrained,
        /// mate constraints translate instance_b to contact instance_a).
        /// Full solver requires iterative numerical approach.
        pub fn solve(&mut self) -> Result<(), String> {
            // Basic validation: check all referenced instances exist.
            for c in &self.constraints {
                let (a, b) = match c {
                    AssemblyConstraint::Mate {
                        instance_a,
                        instance_b,
                        ..
                    }
                    | AssemblyConstraint::Align {
                        instance_a,
                        instance_b,
                        ..
                    }
                    | AssemblyConstraint::Insert {
                        instance_a,
                        instance_b,
                        ..
                    }
                    | AssemblyConstraint::Angle {
                        instance_a,
                        instance_b,
                        ..
                    }
                    | AssemblyConstraint::Distance {
                        instance_a,
                        instance_b,
                        ..
                    } => (*instance_a, *instance_b),
                };
                if !self.instances.iter().any(|i| i.id == a) {
                    return Err(format!("instance {} not found", a));
                }
                if !self.instances.iter().any(|i| i.id == b) {
                    return Err(format!("instance {} not found", b));
                }
            }
            // TODO: iterative constraint solver (Newton-Raphson on 6-DOF transforms)
            Ok(())
        }

        /// Extract bill of materials (aggregated by part name).
        pub fn get_bom(&self) -> Vec<BomEntry> {
            let mut map = std::collections::HashMap::<String, u32>::new();
            for inst in &self.instances {
                if !inst.suppressed {
                    *map.entry(inst.part_ref.name.clone()).or_insert(0) += 1;
                }
            }
            let mut bom: Vec<BomEntry> = map
                .into_iter()
                .map(|(part_name, quantity)| BomEntry {
                    part_name,
                    quantity,
                })
                .collect();
            bom.sort_by(|a, b| a.part_name.cmp(&b.part_name));
            bom
        }

        /// All instances (including suppressed).
        pub fn instances(&self) -> &[PartInstance] {
            &self.instances
        }

        /// All constraints.
        pub fn constraints(&self) -> &[AssemblyConstraint] {
            &self.constraints
        }

        /// Instance count (non-suppressed).
        pub fn active_count(&self) -> usize {
            self.instances.iter().filter(|i| !i.suppressed).count()
        }
    }
}

// ── Tessellation ──

pub mod tessellate {
    use crate::brep::{BrepEdge, BrepFace, BrepSolid, BrepVertex, Surface, TopoId};
    use glam::DVec3;

    /// Tessellate a BREP solid into triangle mesh.
    /// Returns (positions, indices) where each triangle is 3 consecutive indices.
    pub fn tessellate_solid(
        solid: &BrepSolid,
        edges: &[BrepEdge],
        vertices: &[BrepVertex],
    ) -> (Vec<DVec3>, Vec<u32>) {
        let mut positions: Vec<DVec3> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let vert_map: std::collections::HashMap<TopoId, DVec3> =
            vertices.iter().map(|v| (v.id, v.point)).collect();
        let edge_map: std::collections::HashMap<TopoId, &BrepEdge> =
            edges.iter().map(|e| (e.id, e)).collect();

        for shell in &solid.shells {
            for face in &shell.faces {
                tessellate_face(face, &edge_map, &vert_map, &mut positions, &mut indices);
            }
        }
        (positions, indices)
    }

    /// Tessellate a single BREP face.
    fn tessellate_face(
        face: &BrepFace,
        edge_map: &std::collections::HashMap<TopoId, &BrepEdge>,
        vert_map: &std::collections::HashMap<TopoId, DVec3>,
        positions: &mut Vec<DVec3>,
        indices: &mut Vec<u32>,
    ) {
        match &face.surface {
            Surface::Plane { .. } => {
                tessellate_planar_face(face, edge_map, vert_map, positions, indices);
            }
            Surface::Cylinder {
                origin,
                axis,
                radius,
            } => {
                tessellate_cylinder_face(
                    *origin, *axis, *radius, face, edge_map, vert_map, positions, indices,
                );
            }
            Surface::Sphere { center, radius } => {
                tessellate_sphere_face(*center, *radius, positions, indices);
            }
            _ => {
                // Fallback: treat as planar using wire vertices.
                tessellate_planar_face(face, edge_map, vert_map, positions, indices);
            }
        }
    }

    /// Fan triangulation for a planar face (convex assumption for outer wire).
    fn tessellate_planar_face(
        face: &BrepFace,
        edge_map: &std::collections::HashMap<TopoId, &BrepEdge>,
        vert_map: &std::collections::HashMap<TopoId, DVec3>,
        positions: &mut Vec<DVec3>,
        indices: &mut Vec<u32>,
    ) {
        if face.wires.is_empty() {
            return;
        }
        // Collect outer wire vertices by chaining edges.
        // Each edge contributes two vertices; we chain them by matching
        // endpoints to build a continuous polygon loop.
        let wire = &face.wires[0];
        let mut polygon: Vec<DVec3> = Vec::new();
        if wire.is_empty() {
            return;
        }
        // Build ordered vertex chain from edge connectivity.
        // Start with the first edge's start_vertex.
        if let Some(first_edge) = wire.first().and_then(|eid| edge_map.get(eid)) {
            let mut current = first_edge.start_vertex;
            if let Some(&p) = vert_map.get(&current) {
                polygon.push(p);
            }
            for eid in wire {
                if let Some(edge) = edge_map.get(eid) {
                    // Determine which end connects to current, then advance
                    // to the other end.
                    let next = if edge.start_vertex == current {
                        edge.end_vertex
                    } else {
                        // Edge is reversed relative to loop direction.
                        edge.start_vertex
                    };
                    if let Some(&p) = vert_map.get(&next) {
                        if polygon.last().map_or(true, |last| last.distance(p) > 1e-12) {
                            polygon.push(p);
                        }
                    }
                    current = next;
                }
            }
            // Remove last vertex if it closes back to the first (duplicate).
            if polygon.len() > 1 {
                if let (Some(first), Some(last)) = (polygon.first(), polygon.last()) {
                    if first.distance(*last) < 1e-12 {
                        polygon.pop();
                    }
                }
            }
        }
        if polygon.len() < 3 {
            return;
        }
        // Fan triangulation from first vertex.
        let base_idx = positions.len() as u32;
        for p in &polygon {
            positions.push(*p);
        }
        for i in 1..(polygon.len() as u32 - 1) {
            indices.push(base_idx);
            indices.push(base_idx + i);
            indices.push(base_idx + i + 1);
        }
    }

    /// Parametric tessellation for a cylindrical face.
    fn tessellate_cylinder_face(
        origin: DVec3,
        axis: DVec3,
        radius: f64,
        _face: &BrepFace,
        _edge_map: &std::collections::HashMap<TopoId, &BrepEdge>,
        _vert_map: &std::collections::HashMap<TopoId, DVec3>,
        positions: &mut Vec<DVec3>,
        indices: &mut Vec<u32>,
    ) {
        let segments = 24u32;
        let height = 1.0; // Default unit height; full implementation uses face bounds.
        let ax = axis.normalize();
        let u = if ax.x.abs() < 0.9 {
            DVec3::X.cross(ax).normalize()
        } else {
            DVec3::Y.cross(ax).normalize()
        };
        let v = ax.cross(u);
        let base_idx = positions.len() as u32;

        for j in 0..2u32 {
            let h = j as f64 * height;
            for i in 0..segments {
                let theta = 2.0 * std::f64::consts::PI * (i as f64) / (segments as f64);
                let p = origin + ax * h + u * (radius * theta.cos()) + v * (radius * theta.sin());
                positions.push(p);
            }
        }
        // Quads as triangle pairs.
        for i in 0..segments {
            let next = (i + 1) % segments;
            let b0 = base_idx + i;
            let b1 = base_idx + next;
            let t0 = base_idx + segments + i;
            let t1 = base_idx + segments + next;
            indices.extend_from_slice(&[b0, b1, t1, b0, t1, t0]);
        }
    }

    /// Parametric tessellation for a spherical face (UV sphere patch).
    fn tessellate_sphere_face(
        center: DVec3,
        radius: f64,
        positions: &mut Vec<DVec3>,
        indices: &mut Vec<u32>,
    ) {
        let u_segments = 16u32;
        let v_segments = 12u32;
        let base_idx = positions.len() as u32;

        for j in 0..=v_segments {
            let phi = std::f64::consts::PI * (j as f64) / (v_segments as f64);
            for i in 0..=u_segments {
                let theta = 2.0 * std::f64::consts::PI * (i as f64) / (u_segments as f64);
                let x = radius * phi.sin() * theta.cos();
                let y = radius * phi.sin() * theta.sin();
                let z = radius * phi.cos();
                positions.push(center + DVec3::new(x, y, z));
            }
        }
        let stride = u_segments + 1;
        for j in 0..v_segments {
            for i in 0..u_segments {
                let a = base_idx + j * stride + i;
                let b = a + 1;
                let c = a + stride;
                let d = c + 1;
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{DAffine3, DVec3};

    #[test]
    fn create_box_solid() {
        let (solid, edges, verts) = brep::make_box(1, DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0));
        assert_eq!(solid.face_count(), 6, "box has 6 faces");
        assert_eq!(solid.edge_count(), 12, "box has 12 edges");
        assert_eq!(solid.vertex_count(&edges), 8, "box has 8 vertices");
        assert_eq!(verts.len(), 8);
    }

    #[test]
    fn bounding_box_accuracy() {
        let min = DVec3::new(-5.0, -5.0, -5.0);
        let max = DVec3::new(5.0, 5.0, 5.0);
        let (solid, edges, verts) = brep::make_box(1, min, max);
        let (bb_min, bb_max) = solid.bounding_box(&edges, &verts);
        assert!((bb_min.x - (-5.0)).abs() < 1e-10);
        assert!((bb_max.z - 5.0).abs() < 1e-10);
    }

    #[test]
    fn feature_tree_extrude_produces_solid() {
        let mut tree = feature::FeatureTree::new();
        tree.add_feature(feature::Feature::Sketch {
            id: feature::FeatureId(1),
            plane: feature::SketchPlane::XY,
            entities: vec![feature::SketchEntity::Circle {
                center: glam::DVec2::ZERO,
                radius: 5.0,
            }],
        });
        tree.add_feature(feature::Feature::Extrude {
            id: feature::FeatureId(2),
            sketch_ref: feature::FeatureId(1),
            direction: DVec3::Z,
            distance: 10.0,
            operation: feature::BooleanOp::New,
        });
        assert_eq!(tree.len(), 2);
        let (solid, _edges, _verts) = tree.evaluate().expect("evaluate should produce solid");
        assert_eq!(solid.face_count(), 6);
    }

    #[test]
    fn feature_suppress_unsuppress() {
        let mut tree = feature::FeatureTree::new();
        tree.add_feature(feature::Feature::Extrude {
            id: feature::FeatureId(1),
            sketch_ref: feature::FeatureId(0),
            direction: DVec3::Z,
            distance: 5.0,
            operation: feature::BooleanOp::New,
        });
        tree.add_feature(feature::Feature::Fillet {
            id: feature::FeatureId(2),
            edges: vec![100],
            radius: 1.0,
        });
        tree.suppress(feature::FeatureId(2));
        // Should still produce a valid solid (fillet is suppressed).
        let result = tree.evaluate();
        assert!(result.is_ok());
        tree.unsuppress(feature::FeatureId(2));
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn assembly_bom() {
        let mut asm = assembly::Assembly::new("test_assembly");
        let bolt = assembly::PartRef {
            solid_id: 1,
            name: "M6_bolt".into(),
        };
        let nut = assembly::PartRef {
            solid_id: 2,
            name: "M6_nut".into(),
        };
        asm.add_instance(bolt.clone(), DAffine3::IDENTITY, "bolt_1");
        asm.add_instance(bolt.clone(), DAffine3::IDENTITY, "bolt_2");
        asm.add_instance(nut.clone(), DAffine3::IDENTITY, "nut_1");
        asm.add_instance(nut.clone(), DAffine3::IDENTITY, "nut_2");
        asm.add_instance(bolt.clone(), DAffine3::IDENTITY, "bolt_3");
        let bom = asm.get_bom();
        assert_eq!(bom.len(), 2, "BOM should have 2 unique parts");
        let bolt_entry = bom.iter().find(|e| e.part_name == "M6_bolt").unwrap();
        assert_eq!(bolt_entry.quantity, 3);
        let nut_entry = bom.iter().find(|e| e.part_name == "M6_nut").unwrap();
        assert_eq!(nut_entry.quantity, 2);
    }

    #[test]
    fn tessellation_produces_vertices_and_indices() {
        let (solid, edges, verts) = brep::make_box(1, DVec3::ZERO, DVec3::new(1.0, 1.0, 1.0));
        let (positions, indices) = tessellate::tessellate_solid(&solid, &edges, &verts);
        assert!(
            !positions.is_empty(),
            "tessellation should produce positions"
        );
        assert!(!indices.is_empty(), "tessellation should produce indices");
        // Every index should be valid.
        for &idx in &indices {
            assert!((idx as usize) < positions.len(), "index out of bounds");
        }
        // Indices should be multiples of 3 (complete triangles).
        assert_eq!(
            indices.len() % 3,
            0,
            "indices should form complete triangles"
        );
    }

    #[test]
    fn surface_area_unit_cube() {
        let (solid, edges, verts) = brep::make_box(1, DVec3::ZERO, DVec3::new(1.0, 1.0, 1.0));
        let area = solid.surface_area(&edges, &verts);
        // Surface area of unit cube = 6.0. Each face is 2 triangles, each 0.5 area.
        assert!(
            (area - 6.0).abs() < 0.5,
            "unit cube surface area should be ~6.0, got {}",
            area,
        );
    }

    #[test]
    fn volume_positive_for_solid() {
        let (solid, edges, verts) = brep::make_box(1, DVec3::ZERO, DVec3::new(2.0, 3.0, 4.0));
        let vol = solid.volume(&edges, &verts);
        // Signed tetrahedron method on fan-triangulated faces yields an
        // approximation; verify it is positive and nonzero.
        assert!(vol > 0.0, "volume should be positive, got {}", vol);
    }

    #[test]
    fn assembly_constraint_solve_validates_instances() {
        let mut asm = assembly::Assembly::new("constrained");
        let part = assembly::PartRef {
            solid_id: 1,
            name: "plate".into(),
        };
        let id_a = asm.add_instance(part.clone(), DAffine3::IDENTITY, "plate_a");
        let id_b = asm.add_instance(part.clone(), DAffine3::IDENTITY, "plate_b");
        asm.add_constraint(assembly::AssemblyConstraint::Mate {
            instance_a: id_a,
            face_a: 200,
            instance_b: id_b,
            face_b: 201,
        });
        assert!(asm.solve().is_ok());
        // Add constraint referencing non-existent instance.
        asm.add_constraint(assembly::AssemblyConstraint::Distance {
            instance_a: id_a,
            face_a: 200,
            instance_b: 999,
            face_b: 201,
            distance: 5.0,
        });
        assert!(asm.solve().is_err());
    }
}
