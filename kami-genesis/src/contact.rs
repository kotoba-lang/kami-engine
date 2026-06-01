//! contact — rigid contact / collision solver coupled to the 3-D
//! reduced-coordinate articulation (`articulation3d`).
//!
//! Same approach class as PhysX's TGS contact solver (clean-room, no NVIDIA
//! code): velocity-level **sequential impulses / projected Gauss-Seidel** with a
//! Coulomb friction cone, run in the articulation's *joint space*. The
//! contact-space effective inverse mass is the Delassus operator
//! `A = Jₖ M⁻¹ Jₖᵀ`, where `M` is the CRBA joint-space inertia already computed
//! by the dynamics core and `Jₖ` is the contact-point linear Jacobian
//! (`Articulation3dConfig::point_jacobian`). Penetration is corrected with
//! Baumgarte stabilization; restitution is supported (default 0 = inelastic).
//!
//! Collision shapes are spheres / capsules / **boxes** attached to links,
//! resolved against a static ground plane (z = `ground_z`, normal +z), `Plane`
//! / `Aabb` / `Convex` obstacles. A `Collider::Box` emits a **multi-point
//! manifold** (one contact per corner) so a box rests flat and stably rather
//! than balancing on a single deepest point. Broadphase is trivial all-pairs
//! (link counts are small). Self-collision broad/narrow phase is a follow-up.

use crate::articulation3d::{Articulation3dConfig, Articulation3dState, solve_ldlt};
use crate::convex::{ConvexPoly, epa_penetration, gjk_closest_vec};
use glam::Vec3;

#[derive(Clone, Debug)]
pub enum Collider {
    /// Sphere centred at `center` (body frame).
    Sphere { center: Vec3, radius: f32 },
    /// Capsule between `a` and `b` (body frame), swept radius `radius`.
    Capsule { a: Vec3, b: Vec3, radius: f32 },
    /// Oriented box: `center` + half-extents (body frame), optional corner
    /// `radius` (rounded box). Resolved as a **multi-point manifold** — one
    /// contact per corner — so a box rests flat and stably (no wobble) instead
    /// of needing 8 separate sphere colliders.
    Box {
        center: Vec3,
        half: Vec3,
        radius: f32,
    },
}

/// A static environment obstacle, in addition to the implicit ground plane
/// (`ContactParams::ground_z`). This lets a scene box-in a duct / gap / cavity
/// with side and back walls. The PGS solver already resolves contacts with an
/// arbitrary normal, so obstacles compose with the ground at no solver cost.
#[derive(Clone, Debug)]
pub enum Obstacle {
    /// Solid half-space. Free space is the `+normal` side of the plane
    /// `x · normal = offset`; collider centres are pushed back toward `+normal`.
    /// `normal` need not be unit — it is normalised internally.
    Plane { normal: Vec3, offset: f32 },
    /// Solid axis-aligned box `[min, max]`. Colliders are kept outside it
    /// (nearest-face push-out; interior centres exit along the min-penetration
    /// axis).
    Aabb { min: Vec3, max: Vec3 },
    /// Solid arbitrary **convex polytope** (tilted box, hull). Sphere colliders
    /// are resolved against it with GJK (separation) / EPA (penetration), the
    /// general narrow-phase. The convex-vs-convex piece the proxy shapes lacked.
    Convex(ConvexPoly),
}

impl Obstacle {
    /// Contact for a world-space sphere `(c, radius)` belonging to `link`,
    /// or `None` if it is clear of this obstacle (beyond the slop band).
    fn contact(&self, link: usize, c: Vec3, radius: f32, slop: f32) -> Option<Contact> {
        match self {
            Obstacle::Plane { normal, offset } => {
                let n = normal.normalize();
                let d = c.dot(n) - offset; // signed distance, +n = free side
                let depth = radius - d;
                (depth > -slop).then(|| Contact {
                    link,
                    p: c - n * d,
                    n,
                    depth,
                })
            }
            Obstacle::Aabb { min, max } => {
                let cp = c.clamp(*min, *max); // nearest point on/in the box
                let diff = c - cp;
                let dist2 = diff.length_squared();
                if dist2 > 1.0e-12 {
                    let dist = dist2.sqrt();
                    let n = diff / dist;
                    let depth = radius - dist;
                    (depth > -slop).then(|| Contact {
                        link,
                        p: cp,
                        n,
                        depth,
                    })
                } else {
                    // Centre inside the box: exit along the axis of least
                    // penetration (closest face).
                    let dlo = c - *min; // distance to each min face
                    let dhi = *max - c; // distance to each max face
                    let mut best = f32::INFINITY;
                    let mut n = Vec3::Z;
                    for (ax, axis) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
                        if dlo[ax] < best {
                            best = dlo[ax];
                            n = -axis;
                        }
                        if dhi[ax] < best {
                            best = dhi[ax];
                            n = axis;
                        }
                    }
                    Some(Contact {
                        link,
                        p: c,
                        n,
                        depth: radius + best,
                    })
                }
            }
            Obstacle::Convex(poly) => {
                // sphere centre as a degenerate (1-vertex) convex; GJK gives the
                // separation vector from the polytope toward the centre.
                let pt = ConvexPoly::new(vec![c]);
                let cv = gjk_closest_vec(&pt, poly);
                let d = cv.length();
                if d > 1e-6 {
                    let n = cv / d; // poly → centre (push-out direction)
                    let depth = radius - d;
                    (depth > -slop).then(|| Contact {
                        link,
                        p: c - n * radius,
                        n,
                        depth,
                    })
                } else {
                    // centre inside the polytope → EPA for the exit direction.
                    let (pd, mut n) = epa_penetration(&pt, poly)?;
                    let centroid =
                        poly.verts.iter().copied().sum::<Vec3>() / poly.verts.len().max(1) as f32;
                    if n.dot(c - centroid) < 0.0 {
                        n = -n; // orient outward (away from the polytope interior)
                    }
                    Some(Contact {
                        link,
                        p: c,
                        n,
                        depth: radius + pd,
                    })
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContactParams {
    pub ground_z: f32,
    pub restitution: f32, // 0 = inelastic
    pub friction: f32,    // Coulomb μ
    pub baumgarte: f32,   // position-error feedback gain (0.1–0.2)
    pub slop: f32,        // penetration allowance before push-out
    pub iters: usize,     // PGS sweeps
}

impl Default for ContactParams {
    fn default() -> Self {
        Self {
            ground_z: 0.0,
            restitution: 0.0,
            friction: 0.8,
            baumgarte: 0.15,
            slop: 1.0e-3,
            iters: 12,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContactWorld {
    /// `(body index, collider in that body's frame)`.
    pub colliders: Vec<(usize, Collider)>,
    pub params: ContactParams,
    /// Static environment obstacles resolved in addition to the ground plane.
    pub obstacles: Vec<Obstacle>,
}

#[derive(Clone, Copy, Debug)]
struct Contact {
    link: usize,
    p: Vec3,    // world contact point
    n: Vec3,    // world normal (ground → body), unit
    depth: f32, // penetration (> 0 if overlapping)
}

impl ContactWorld {
    pub fn new(colliders: Vec<(usize, Collider)>, params: ContactParams) -> Self {
        Self {
            colliders,
            params,
            obstacles: Vec::new(),
        }
    }

    /// Add static environment obstacles (walls / boxes) for boxing-in a
    /// duct / gap / cavity. Composes with the implicit ground plane.
    pub fn with_obstacles(mut self, obstacles: Vec<Obstacle>) -> Self {
        self.obstacles = obstacles;
        self
    }

    /// Step the articulation one `dt` with contact resolution:
    /// predict free velocity → generate contacts → PGS velocity solve →
    /// integrate positions.
    pub fn step(
        &self,
        cfg: &Articulation3dConfig,
        st: &mut Articulation3dState,
        tau_applied: &[f32],
    ) {
        let (qddot, m) = cfg.forward_dynamics(st, tau_applied);
        // Semi-implicit velocity prediction.
        let dt = cfg.dt;
        for d in 0..cfg.ndof {
            st.qdot[d] += dt * qddot[d];
        }
        let contacts = self.generate(cfg, &st.q);
        if !contacts.is_empty() {
            self.solve_velocity(cfg, &m, st, &contacts);
        }
        cfg.integrate_positions(st);
    }

    /// Number of live ground contacts at the current pose (test/inspection).
    pub fn contact_count(&self, cfg: &Articulation3dConfig, q: &[f32]) -> usize {
        self.generate(cfg, q).len()
    }

    fn generate(&self, cfg: &Articulation3dConfig, q: &[f32]) -> Vec<Contact> {
        let lw = cfg.link_world(q);
        let gz = self.params.ground_z;
        let mut out = Vec::new();
        for (body, col) in &self.colliders {
            let (r, p0) = lw[*body];
            let mut probe = |center_body: Vec3, radius: f32| {
                let c = p0 + r * center_body;
                let depth = (gz + radius) - c.z;
                if depth > -self.params.slop {
                    out.push(Contact {
                        link: *body,
                        p: Vec3::new(c.x, c.y, gz), // contact on the plane
                        n: Vec3::Z,
                        depth,
                    });
                }
                for ob in &self.obstacles {
                    if let Some(ct) = ob.contact(*body, c, radius, self.params.slop) {
                        out.push(ct);
                    }
                }
            };
            match col {
                Collider::Sphere { center, radius } => probe(*center, *radius),
                Collider::Capsule { a, b, radius } => {
                    // Sample the capsule axis with spacing ≤ radius so no contact
                    // along the segment (not just the two endpoints) is missed.
                    let seg = *b - *a;
                    let len = seg.length();
                    let n_samp = ((len / radius.max(1.0e-4)).ceil() as usize + 1).max(2);
                    for s in 0..n_samp {
                        let t = s as f32 / (n_samp - 1) as f32;
                        probe(*a + seg * t, *radius);
                    }
                }
                Collider::Box {
                    center,
                    half,
                    radius,
                } => {
                    // 8 corners → a multi-point contact manifold. Resting on the
                    // four lower corners keeps the box flat and stable.
                    for sx in [-1.0f32, 1.0] {
                        for sy in [-1.0f32, 1.0] {
                            for sz in [-1.0f32, 1.0] {
                                let corner =
                                    *center + Vec3::new(sx * half.x, sy * half.y, sz * half.z);
                                probe(corner, *radius);
                            }
                        }
                    }
                }
            }
        }
        out
    }

    fn solve_velocity(
        &self,
        cfg: &Articulation3dConfig,
        m: &[Vec<f32>],
        st: &mut Articulation3dState,
        contacts: &[Contact],
    ) {
        let n = cfg.ndof;
        let dt = cfg.dt;
        // Per-contact rows + Delassus diagonals (precomputed once).
        struct Row {
            jn: Vec<f32>,
            jt1: Vec<f32>,
            jt2: Vec<f32>,
            minv_jn: Vec<f32>,
            minv_jt1: Vec<f32>,
            minv_jt2: Vec<f32>,
            inv_mn: f32,
            inv_mt1: f32,
            inv_mt2: f32,
            bias_n: f32,
        }
        let mut rows = Vec::with_capacity(contacts.len());
        for c in contacts {
            let (t1, t2) = tangents(c.n);
            let pj = cfg.point_jacobian(c.link, c.p, &st.q);
            let jn = project(&pj, c.n, n);
            let jt1 = project(&pj, t1, n);
            let jt2 = project(&pj, t2, n);
            let minv_jn = m_inv_mul(m, &jn);
            let minv_jt1 = m_inv_mul(m, &jt1);
            let minv_jt2 = m_inv_mul(m, &jt2);
            let mn = dotv(&jn, &minv_jn).max(1e-9);
            let mt1 = dotv(&jt1, &minv_jt1).max(1e-9);
            let mt2 = dotv(&jt2, &minv_jt2).max(1e-9);
            // Penetration push-out (Baumgarte), capped at the slop band.
            let pen = (c.depth - self.params.slop).max(0.0);
            let vn_pre = dotv(&jn, &st.qdot);
            let restitution = if vn_pre < 0.0 {
                -self.params.restitution * vn_pre
            } else {
                0.0
            };
            let bias_n = (self.params.baumgarte / dt) * pen + restitution;
            rows.push(Row {
                jn,
                jt1,
                jt2,
                minv_jn,
                minv_jt1,
                minv_jt2,
                inv_mn: 1.0 / mn,
                inv_mt1: 1.0 / mt1,
                inv_mt2: 1.0 / mt2,
                bias_n,
            });
        }

        let mu = self.params.friction;
        let mut lam_n = vec![0.0_f32; contacts.len()];
        let mut lam_t1 = vec![0.0_f32; contacts.len()];
        let mut lam_t2 = vec![0.0_f32; contacts.len()];

        for _ in 0..self.params.iters {
            for k in 0..rows.len() {
                let row = &rows[k];
                // Normal: drive vn toward the separating target (bias_n ≥ 0).
                let vn = dotv(&row.jn, &st.qdot);
                let mut dln = (row.bias_n - vn) * row.inv_mn;
                let new_n = (lam_n[k] + dln).max(0.0);
                dln = new_n - lam_n[k];
                lam_n[k] = new_n;
                axpy_inplace(&mut st.qdot, dln, &row.minv_jn);

                // Friction: clamp tangential impulse to the cone |λt| ≤ μ λn.
                let lim = mu * lam_n[k];
                let vt1 = dotv(&row.jt1, &st.qdot);
                let mut dlt1 = (-vt1) * row.inv_mt1;
                let new_t1 = (lam_t1[k] + dlt1).clamp(-lim, lim);
                dlt1 = new_t1 - lam_t1[k];
                lam_t1[k] = new_t1;
                axpy_inplace(&mut st.qdot, dlt1, &row.minv_jt1);

                let vt2 = dotv(&row.jt2, &st.qdot);
                let mut dlt2 = (-vt2) * row.inv_mt2;
                let new_t2 = (lam_t2[k] + dlt2).clamp(-lim, lim);
                dlt2 = new_t2 - lam_t2[k];
                lam_t2[k] = new_t2;
                axpy_inplace(&mut st.qdot, dlt2, &row.minv_jt2);
            }
        }
    }
}

/// Project a per-DOF linear Jacobian (`pj[d] = ∂vₚ/∂q̇_d`) onto direction `dir`.
fn project(pj: &[[f32; 3]], dir: Vec3, ndof: usize) -> Vec<f32> {
    let mut row = vec![0.0_f32; ndof];
    for d in 0..ndof {
        row[d] = dir.x * pj[d][0] + dir.y * pj[d][1] + dir.z * pj[d][2];
    }
    row
}

fn m_inv_mul(m: &[Vec<f32>], j: &[f32]) -> Vec<f32> {
    let mut b = j.to_vec();
    solve_ldlt(m, &mut b).unwrap_or_else(|| vec![0.0; j.len()])
}

fn dotv(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn axpy_inplace(y: &mut [f32], s: f32, x: &[f32]) {
    for d in 0..y.len() {
        y[d] += s * x[d];
    }
}

/// Two orthonormal tangents spanning the plane ⟂ to unit `n`.
fn tangents(n: Vec3) -> (Vec3, Vec3) {
    let a = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let t1 = (a - n * a.dot(n)).normalize();
    let t2 = n.cross(t1);
    (t1, t2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::articulation3d::{Articulation3dConfig, Articulation3dState, Body3d, JointType3d};
    use crate::spatial::spatial_inertia;
    use glam::Mat3;

    /// A single revolute link (about +y) hanging under gravity, with a sphere
    /// collider at its tip and a ground plane partway down. The tip must come
    /// to rest **on** the plane: no run-away penetration, no residual motion.
    fn one_link_with_ground(ground_z: f32) -> (Articulation3dConfig, ContactWorld) {
        let m = 1.0;
        let l = 1.0;
        let i_perp = m * l * l / 12.0;
        let i_com = Mat3::from_diagonal(Vec3::new(i_perp, i_perp, 0.0));
        let com = Vec3::new(0.0, 0.0, -l / 2.0);
        let body = Body3d {
            name: "link".into(),
            parent: -1,
            joint_type: JointType3d::Revolute,
            axis: Vec3::new(0.0, -1.0, 0.0),
            e_tree: Mat3::IDENTITY,
            r_tree: Vec3::ZERO,
            inertia: spatial_inertia(m, com, i_com),
            mass: m,
            com,
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof: 0,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![body],
            gravity: Vec3::new(0.0, 0.0, -9.81),
            dt: 1.0 / 240.0,
            ndof: 1,
        };
        let cw = ContactWorld::new(
            vec![(
                0,
                Collider::Sphere {
                    center: Vec3::new(0.0, 0.0, -l),
                    radius: 0.05,
                },
            )],
            ContactParams {
                ground_z,
                ..Default::default()
            },
        );
        (cfg, cw)
    }

    fn tip_z(cfg: &Articulation3dConfig, st: &Articulation3dState) -> f32 {
        let (r, p0) = cfg.link_world(&st.q)[0];
        (p0 + r * Vec3::new(0.0, 0.0, -1.0)).z
    }

    #[test]
    fn link_settles_on_ground_without_penetrating() {
        // Start horizontal (q=π/2 about −y swings tip toward −x/down). Ground
        // at z=−0.6 catches the tip (radius 0.05 → rest near −0.55).
        let (cfg, cw) = one_link_with_ground(-0.6);
        let mut st = Articulation3dState {
            q: vec![std::f32::consts::FRAC_PI_2],
            qdot: vec![0.0],
        };
        for _ in 0..2000 {
            cw.step(&cfg, &mut st, &[0.0]);
        }
        let z = tip_z(&cfg, &st);
        // Tip rests at/above ground minus slop; sphere radius keeps center above.
        assert!(z >= -0.62, "tip penetrated: z={z}");
        assert!(z <= -0.45, "tip should have fallen to the ground, z={z}");
        // At rest: joint velocity ≈ 0.
        assert!(
            st.qdot[0].abs() < 0.05,
            "should be at rest, qdot={}",
            st.qdot[0]
        );
    }

    #[test]
    fn no_contact_when_ground_is_far_below() {
        let (cfg, cw) = one_link_with_ground(-5.0);
        let st = Articulation3dState {
            q: vec![std::f32::consts::FRAC_PI_2],
            qdot: vec![0.0],
        };
        assert_eq!(cw.contact_count(&cfg, &st.q), 0);
    }

    #[test]
    fn side_wall_obstacle_stops_a_swinging_link() {
        // The single revolute link swings under gravity from horizontal. A
        // vertical side-wall half-space (normal +x, surface at x = 0.3) blocks
        // the tip's −x... here we instead place the wall so the swinging tip
        // (which moves toward −x as it falls about −y) is caught: free side is
        // −x (normal −x, offset −0.3 → plane x = −0.3, free side x < −0.3 is
        // solid? no). Use normal +x, offset −0.3: free side x > −0.3; the tip
        // must not pass below x = −0.3 by more than slop + radius.
        let m = 1.0;
        let l = 1.0;
        let i_perp = m * l * l / 12.0;
        let i_com = Mat3::from_diagonal(Vec3::new(i_perp, i_perp, 0.0));
        let com = Vec3::new(0.0, 0.0, -l / 2.0);
        let body = Body3d {
            name: "link".into(),
            parent: -1,
            joint_type: JointType3d::Revolute,
            axis: Vec3::new(0.0, -1.0, 0.0),
            e_tree: Mat3::IDENTITY,
            r_tree: Vec3::ZERO,
            inertia: spatial_inertia(m, com, i_com),
            mass: m,
            com,
            lower: 0.0,
            upper: 0.0,
            has_limit: false,
            effort: 0.0,
            damping: 0.0,
            dof: 0,
        };
        let cfg = Articulation3dConfig {
            bodies: vec![body],
            gravity: Vec3::new(0.0, 0.0, -9.81),
            dt: 1.0 / 240.0,
            ndof: 1,
        };
        // Tip sphere; ground far below so only the wall matters.
        let radius = 0.05;
        let cw = ContactWorld::new(
            vec![(
                0,
                Collider::Sphere {
                    center: Vec3::new(0.0, 0.0, -l),
                    radius,
                },
            )],
            ContactParams {
                ground_z: -10.0,
                ..Default::default()
            },
        )
        .with_obstacles(vec![Obstacle::Plane {
            normal: Vec3::X,
            offset: -0.30,
        }]);
        // Start so the tip is on the free side and swings toward the wall.
        let mut st = Articulation3dState {
            q: vec![std::f32::consts::FRAC_PI_2],
            qdot: vec![0.0],
        };
        let tip_x = |st: &Articulation3dState| {
            let (r, p0) = cfg.link_world(&st.q)[0];
            (p0 + r * Vec3::new(0.0, 0.0, -1.0)).x
        };
        for _ in 0..3000 {
            cw.step(&cfg, &mut st, &[0.0]);
        }
        // Wall holds the tip centre at x ≥ -0.30 - radius (minus a little slop).
        assert!(
            tip_x(&st) >= -0.30 - radius - 0.02,
            "wall breached: x={}",
            tip_x(&st)
        );
        assert!(st.q.iter().all(|v| v.is_finite()) && st.qdot.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn aabb_obstacle_keeps_sphere_outside() {
        // A static AABB obstacle yields an outward contact for a sphere that
        // would otherwise overlap it.
        let ob = Obstacle::Aabb {
            min: Vec3::new(-1.0, -1.0, -1.0),
            max: Vec3::new(0.0, 1.0, 1.0),
        };
        // Sphere centre just to the +x side of the box face at x=0.
        let ct = ob
            .contact(7, Vec3::new(0.03, 0.0, 0.0), 0.05, 1.0e-3)
            .expect("overlap → contact");
        assert_eq!(ct.link, 7);
        assert!(
            ct.n.dot(Vec3::X) > 0.9,
            "normal should push out +x: n={:?}",
            ct.n
        );
        assert!(ct.depth > 0.0, "penetration positive");
        // Far away → no contact.
        assert!(
            ob.contact(7, Vec3::new(0.5, 0.0, 0.0), 0.05, 1.0e-3)
                .is_none()
        );
    }

    #[test]
    fn restitution_controls_the_rebound_height() {
        // A sphere dropped from z=1 onto the ground: with restitution e=0.6 it
        // bounces back high (but below the drop, since e<1); with e=0 it does not.
        let urdf = r#"<robot name="d">
<link name="world"/>
<joint name="jz" type="prismatic"><parent link="world"/><child link="body"/><origin xyz="0 0 0"/><axis xyz="0 0 1"/><limit lower="-1e4" upper="1e4" effort="1e9" velocity="1e4"/></joint>
<link name="body"><inertial><origin xyz="0 0 0"/><mass value="5"/><inertia ixx="0.05" iyy="0.05" izz="0.05" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;
        let sys = kami_articulated::parse_urdf(urdf).expect("urdf");
        let cfg = Articulation3dConfig::from_articulated_system(
            &sys,
            Vec3::new(0.0, 0.0, -9.81),
            1.0 / 240.0,
        );
        let body = cfg.body_index("body").expect("body");
        let radius = 0.1;

        let first_bounce_apex = |e: f32| -> f32 {
            let cw = ContactWorld::new(
                vec![(
                    body,
                    Collider::Sphere {
                        center: Vec3::ZERO,
                        radius,
                    },
                )],
                ContactParams {
                    ground_z: 0.0,
                    restitution: e,
                    friction: 0.0,
                    ..Default::default()
                },
            );
            let mut st = Articulation3dState::zeros(cfg.ndof);
            st.q[0] = 1.0; // centre at z = 1 (drop distance ≈ 0.9 above the rest)
            let (mut hit, mut apex, mut done) = (false, 0.0_f32, false);
            for _ in 0..2400 {
                cw.step(&cfg, &mut st, &[0.0]);
                let z = st.q[0];
                if !hit {
                    if z < radius + 0.02 {
                        hit = true;
                    }
                } else if !done {
                    apex = apex.max(z);
                    if z < radius + 0.02 && apex > radius + 0.03 {
                        done = true; // returned to the ground → end of first bounce
                    }
                }
            }
            apex
        };

        let bouncy = first_bounce_apex(0.6);
        let dead = first_bounce_apex(0.0);
        assert!(bouncy > 0.3, "e=0.6 should rebound high: apex={bouncy}");
        assert!(
            bouncy < 1.0,
            "e<1 must not exceed the drop height: apex={bouncy}"
        );
        assert!(dead < 0.18, "e=0 should barely rebound: apex={dead}");
        assert!(
            bouncy > dead + 0.2,
            "restitution had no effect: {dead} vs {bouncy}"
        );
    }

    #[test]
    fn friction_cone_holds_below_angle_and_slides_above() {
        // A sphere on a plane tilted by θ stays (static friction) when
        // tanθ < μ and slides down-slope when tanθ > μ — the Coulomb-cone law.
        let urdf = r#"<robot name="s">
<link name="world"/>
<joint name="jx" type="prismatic"><parent link="world"/><child link="lx"/><origin xyz="0 0 0"/><axis xyz="1 0 0"/><limit lower="-1e4" upper="1e4" effort="1e9" velocity="1e4"/></joint>
<link name="lx"><inertial><mass value="0.0001"/><inertia ixx="1e-7" iyy="1e-7" izz="1e-7" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="jy" type="prismatic"><parent link="lx"/><child link="ly"/><origin xyz="0 0 0"/><axis xyz="0 1 0"/><limit lower="-1e4" upper="1e4" effort="1e9" velocity="1e4"/></joint>
<link name="ly"><inertial><mass value="0.0001"/><inertia ixx="1e-7" iyy="1e-7" izz="1e-7" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="jz" type="prismatic"><parent link="ly"/><child link="body"/><origin xyz="0 0 0"/><axis xyz="0 0 1"/><limit lower="-1e4" upper="1e4" effort="1e9" velocity="1e4"/></joint>
<link name="body"><inertial><origin xyz="0 0 0"/><mass value="10"/><inertia ixx="0.1" iyy="0.1" izz="0.1" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;
        let sys = kami_articulated::parse_urdf(urdf).expect("urdf");
        let cfg = Articulation3dConfig::from_articulated_system(
            &sys,
            Vec3::new(0.0, 0.0, -9.81),
            1.0 / 240.0,
        );
        let body = cfg.body_index("body").expect("body");
        let r = 0.2;
        let mu = 0.8; // friction angle ≈ 38.7°

        let down_slope_drift = |theta_deg: f32| -> f32 {
            let th = theta_deg.to_radians();
            let (s, c) = (th.sin(), th.cos());
            let n = Vec3::new(s, 0.0, c); // plane normal (tilted about y)
            let cw = ContactWorld::new(
                vec![(
                    body,
                    Collider::Sphere {
                        center: Vec3::ZERO,
                        radius: r,
                    },
                )],
                ContactParams {
                    ground_z: -100.0,
                    friction: mu,
                    ..Default::default()
                },
            )
            .with_obstacles(vec![Obstacle::Plane {
                normal: n,
                offset: 0.0,
            }]);
            let mut st = Articulation3dState::zeros(cfg.ndof);
            let start = n * r; // rest the sphere on the plane (centre at distance r)
            st.q[0] = start.x;
            st.q[1] = start.y;
            st.q[2] = start.z;
            for _ in 0..800 {
                cw.step(&cfg, &mut st, &[0.0, 0.0, 0.0]);
            }
            let center = Vec3::new(st.q[0], st.q[1], st.q[2]);
            let t = Vec3::new(c, 0.0, -s); // unit down-slope direction
            (center - start).dot(t)
        };

        let stays = down_slope_drift(20.0); // tan20°=0.36 < 0.8 → static
        let slides = down_slope_drift(55.0); // tan55°=1.43 > 0.8 → slides
        assert!(
            stays.abs() < 0.05,
            "should stick on a 20° slope: drift={stays}"
        );
        assert!(
            slides > 0.5,
            "should slide down a 55° slope: drift={slides}"
        );
    }

    #[test]
    fn capsule_mid_span_contact_is_not_missed() {
        // A long horizontal capsule whose two endpoints clear an AABB obstacle
        // but whose MIDDLE presses on it. Endpoint-only sampling would miss it;
        // the axis sampling catches it.
        let urdf = r#"<robot name="c">
<link name="world"/>
<joint name="jx" type="prismatic"><parent link="world"/><child link="body"/><origin xyz="0 0 0"/><axis xyz="1 0 0"/><limit lower="-100" upper="100" effort="1e8" velocity="1000"/></joint>
<link name="body"><inertial><origin xyz="0 0 0"/><mass value="10"/><inertia ixx="1" iyy="1" izz="1" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;
        let sys = kami_articulated::parse_urdf(urdf).expect("urdf");
        let cfg = Articulation3dConfig::from_articulated_system(
            &sys,
            Vec3::new(0.0, 0.0, -9.81),
            1.0 / 240.0,
        );
        let body = cfg.body_index("body").expect("body");
        // capsule along x at z = 0.5, half-length 1.0, radius 0.2.
        let cw = ContactWorld::new(
            vec![(
                body,
                Collider::Capsule {
                    a: Vec3::new(-1.0, 0.0, 0.5),
                    b: Vec3::new(1.0, 0.0, 0.5),
                    radius: 0.2,
                },
            )],
            ContactParams {
                ground_z: -10.0,
                ..Default::default()
            }, // ground far below
        )
        .with_obstacles(vec![Obstacle::Aabb {
            min: Vec3::new(-0.3, -0.3, 0.0),
            max: Vec3::new(0.3, 0.3, 0.45), // top at z=0.45; capsule mid bottom = 0.3
        }]);
        let st = Articulation3dState::zeros(cfg.ndof);
        // the two endpoints (x=±1) are clear; only the mid-span samples contact.
        assert!(
            cw.contact_count(&cfg, &st.q) >= 1,
            "mid-span capsule contact missed"
        );
    }

    #[test]
    fn box_collider_makes_a_four_point_manifold_and_rests() {
        // A 4-DOF floating base (x,y,z + yaw) carrying a Box collider. The box
        // stays axis-aligned, so its 4 lower corners form a stable manifold —
        // it rests flat on the ground instead of balancing on one point.
        let urdf = r#"<robot name="b">
<link name="world"/>
<joint name="jx" type="prismatic"><parent link="world"/><child link="lx"/><origin xyz="0 0 0"/><axis xyz="1 0 0"/><limit lower="-100" upper="100" effort="1e8" velocity="1000"/></joint>
<link name="lx"><inertial><mass value="0.0001"/><inertia ixx="1e-7" iyy="1e-7" izz="1e-7" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="jy" type="prismatic"><parent link="lx"/><child link="ly"/><origin xyz="0 0 0"/><axis xyz="0 1 0"/><limit lower="-100" upper="100" effort="1e8" velocity="1000"/></joint>
<link name="ly"><inertial><mass value="0.0001"/><inertia ixx="1e-7" iyy="1e-7" izz="1e-7" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="jz" type="prismatic"><parent link="ly"/><child link="lz"/><origin xyz="0 0 0"/><axis xyz="0 0 1"/><limit lower="-100" upper="100" effort="1e8" velocity="1000"/></joint>
<link name="lz"><inertial><mass value="0.0001"/><inertia ixx="1e-7" iyy="1e-7" izz="1e-7" ixy="0" ixz="0" iyz="0"/></inertial></link>
<joint name="jyaw" type="continuous"><parent link="lz"/><child link="body"/><origin xyz="0 0 0"/><axis xyz="0 0 1"/></joint>
<link name="body"><inertial><origin xyz="0 0 0"/><mass value="50"/><inertia ixx="8" iyy="8" izz="8" ixy="0" ixz="0" iyz="0"/></inertial></link>
</robot>"#;
        let sys = kami_articulated::parse_urdf(urdf).expect("urdf");
        let cfg = Articulation3dConfig::from_articulated_system(
            &sys,
            Vec3::new(0.0, 0.0, -9.81),
            1.0 / 240.0,
        );
        let body = cfg.body_index("body").expect("body");
        let half = Vec3::splat(0.5);
        let cw = ContactWorld::new(
            vec![(
                body,
                Collider::Box {
                    center: Vec3::ZERO,
                    half,
                    radius: 0.0,
                },
            )],
            ContactParams {
                ground_z: 0.0,
                friction: 1.0,
                ..Default::default()
            },
        );

        // placed with the box centre at z = 0.4 → 4 lower corners below ground.
        let mut st = Articulation3dState::zeros(cfg.ndof);
        st.q[2] = 0.4;
        assert_eq!(
            cw.contact_count(&cfg, &st.q),
            4,
            "expected a 4-corner manifold"
        );

        // dropped from z = 2, it falls and rests flat (centre ≈ half-height).
        st = Articulation3dState::zeros(cfg.ndof);
        st.q[2] = 2.0;
        for _ in 0..3000 {
            cw.step(&cfg, &mut st, &vec![0.0; cfg.ndof]);
        }
        assert!(
            st.q[2] > 0.35 && st.q[2] < 0.65,
            "did not rest flat: z={}",
            st.q[2]
        );
        assert!(st.qdot[2].abs() < 0.2, "still moving: {}", st.qdot[2]);
        assert!(st.q.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn convex_obstacle_resolves_sphere_via_gjk_epa() {
        use glam::Quat;
        // a 45°-tilted box (corner points along +x at ~0.707) as a Convex obstacle.
        let poly = ConvexPoly::box_at(Vec3::ZERO, Vec3::splat(0.5), Quat::from_rotation_z(0.785));
        let ob = Obstacle::Convex(poly);
        // far away → no contact
        assert!(
            ob.contact(3, Vec3::new(3.0, 0.0, 0.0), 0.1, 1.0e-3)
                .is_none()
        );
        // just outside the +x corner, within the sphere radius → outward contact
        let ct = ob
            .contact(3, Vec3::new(0.78, 0.0, 0.0), 0.1, 1.0e-3)
            .expect("near corner → contact (GJK)");
        assert_eq!(ct.link, 3);
        assert!(ct.depth > 0.0 && ct.depth.is_finite(), "depth={}", ct.depth);
        assert!(ct.n.x > 0.3, "normal should push out +x: n={:?}", ct.n);
        // centre inside the polytope → EPA push-out (large depth)
        let ci = ob
            .contact(3, Vec3::ZERO, 0.1, 1.0e-3)
            .expect("inside → contact (EPA)");
        assert!(
            ci.depth > 0.1 && ci.depth.is_finite(),
            "epa depth={}",
            ci.depth
        );
    }

    #[test]
    fn contact_does_not_inject_energy() {
        // With restitution 0, total energy must not rise over the contact phase
        // (Baumgarte can add a little; bound generously but finite).
        let (cfg, cw) = one_link_with_ground(-0.6);
        let mut st = Articulation3dState {
            q: vec![std::f32::consts::FRAC_PI_2],
            qdot: vec![0.0],
        };
        let e0 = cfg.energy(&st);
        let mut emax = e0;
        for _ in 0..2000 {
            cw.step(&cfg, &mut st, &[0.0]);
            emax = emax.max(cfg.energy(&st));
        }
        assert!(
            emax <= e0 + 0.05 * e0.abs().max(1.0),
            "energy grew: e0={e0} emax={emax}"
        );
    }
}
