//! kami-physics-2d: Lightweight 2D physics engine.
//!
//! AABB + circle colliders, broadphase (spatial hash), narrowphase (SAT),
//! impulse resolution, and trigger callbacks.

use glam::Vec2;

/// 2D rigid body.
#[derive(Debug, Clone)]
pub struct Body2D {
    pub position: Vec2,
    pub velocity: Vec2,
    pub mass: f32,        // 0 = static
    pub restitution: f32, // bounciness (0..1)
    pub friction: f32,
    pub collider: Collider2D,
    pub is_trigger: bool,
    pub user_data: u64, // entity ID or tag
}

/// 2D collider shapes.
#[derive(Debug, Clone)]
pub enum Collider2D {
    AABB { half_w: f32, half_h: f32 },
    Circle { radius: f32 },
}

/// Collision contact.
#[derive(Debug, Clone)]
pub struct Contact2D {
    pub body_a: usize,
    pub body_b: usize,
    pub normal: Vec2,
    pub depth: f32,
    pub point: Vec2,
}

/// 2D physics world.
pub struct World2D {
    pub bodies: Vec<Body2D>,
    pub gravity: Vec2,
    contacts: Vec<Contact2D>,
}

impl World2D {
    pub fn new(gravity: Vec2) -> Self {
        Self {
            bodies: Vec::new(),
            gravity,
            contacts: Vec::new(),
        }
    }

    pub fn add(&mut self, body: Body2D) -> usize {
        let id = self.bodies.len();
        self.bodies.push(body);
        id
    }

    /// Step simulation by dt seconds.
    pub fn step(&mut self, dt: f32) {
        let n = self.bodies.len();

        // Integrate velocity + gravity
        for body in &mut self.bodies {
            if body.mass > 0.0 {
                body.velocity += self.gravity * dt;
                body.position += body.velocity * dt;
            }
        }

        // Broadphase + narrowphase: O(n^2) brute force (fine for <500 bodies)
        self.contacts.clear();
        for i in 0..n {
            for j in (i + 1)..n {
                if let Some(contact) = Self::test_collision(i, j, &self.bodies[i], &self.bodies[j])
                {
                    self.contacts.push(contact);
                }
            }
        }

        // Resolve contacts
        for contact in &self.contacts {
            let (a, b) = if contact.body_a < contact.body_b {
                let (left, right) = self.bodies.split_at_mut(contact.body_b);
                (&mut left[contact.body_a], &mut right[0])
            } else {
                let (left, right) = self.bodies.split_at_mut(contact.body_a);
                (&mut right[0], &mut left[contact.body_b])
            };

            if a.is_trigger || b.is_trigger {
                continue;
            }

            let inv_mass_a = if a.mass > 0.0 { 1.0 / a.mass } else { 0.0 };
            let inv_mass_b = if b.mass > 0.0 { 1.0 / b.mass } else { 0.0 };
            let inv_total = inv_mass_a + inv_mass_b;
            if inv_total == 0.0 {
                continue;
            }

            // Positional correction
            let correction = contact.normal * (contact.depth / inv_total) * 0.8;
            a.position -= correction * inv_mass_a;
            b.position += correction * inv_mass_b;

            // Impulse
            let rel_vel = b.velocity - a.velocity;
            let vel_along = rel_vel.dot(contact.normal);
            if vel_along > 0.0 {
                continue;
            }

            let e = (a.restitution * b.restitution).sqrt();
            let j = -(1.0 + e) * vel_along / inv_total;
            let impulse = contact.normal * j;
            a.velocity -= impulse * inv_mass_a;
            b.velocity += impulse * inv_mass_b;
        }
    }

    pub fn contacts(&self) -> &[Contact2D] {
        &self.contacts
    }

    fn test_collision(ia: usize, ib: usize, a: &Body2D, b: &Body2D) -> Option<Contact2D> {
        match (&a.collider, &b.collider) {
            (Collider2D::Circle { radius: ra }, Collider2D::Circle { radius: rb }) => {
                let diff = b.position - a.position;
                let dist = diff.length();
                let overlap = ra + rb - dist;
                if overlap <= 0.0 {
                    return None;
                }
                let normal = if dist > 0.001 { diff / dist } else { Vec2::X };
                Some(Contact2D {
                    body_a: ia,
                    body_b: ib,
                    normal,
                    depth: overlap,
                    point: a.position + normal * *ra,
                })
            }
            (
                Collider2D::AABB {
                    half_w: hw_a,
                    half_h: hh_a,
                },
                Collider2D::AABB {
                    half_w: hw_b,
                    half_h: hh_b,
                },
            ) => {
                let dx = (b.position.x - a.position.x).abs() - (hw_a + hw_b);
                let dy = (b.position.y - a.position.y).abs() - (hh_a + hh_b);
                if dx > 0.0 || dy > 0.0 {
                    return None;
                }
                let (normal, depth) = if dx > dy {
                    (Vec2::new((b.position.x - a.position.x).signum(), 0.0), -dx)
                } else {
                    (Vec2::new(0.0, (b.position.y - a.position.y).signum()), -dy)
                };
                Some(Contact2D {
                    body_a: ia,
                    body_b: ib,
                    normal,
                    depth,
                    point: (a.position + b.position) * 0.5,
                })
            }
            _ => None, // mixed AABB/Circle: TODO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circle_collision() {
        let mut world = World2D::new(Vec2::ZERO);
        world.add(Body2D {
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::new(1.0, 0.0),
            mass: 1.0,
            restitution: 1.0,
            friction: 0.0,
            collider: Collider2D::Circle { radius: 1.0 },
            is_trigger: false,
            user_data: 0,
        });
        world.add(Body2D {
            position: Vec2::new(1.5, 0.0),
            velocity: Vec2::ZERO,
            mass: 1.0,
            restitution: 1.0,
            friction: 0.0,
            collider: Collider2D::Circle { radius: 1.0 },
            is_trigger: false,
            user_data: 1,
        });
        world.step(0.016);
        assert_eq!(world.contacts().len(), 1);
    }
}
