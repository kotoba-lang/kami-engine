//! P26-benchmark-ns-vs-rule — compare M1 rule-based fire propagation
//! against the v3 Navier-Stokes / DEC stack on a fixed voxel scene.
//!
//! Fixture: 32×32×32 paper slab with a 3×3×3 fire seed at the centre.
//!
//! M1 (rule-based):
//!   every tick, for each paper cell, if any of its 6 neighbours is
//!   fire, ignite it (swap to fire).
//!
//! v3-NS (DEC):
//!   fire cells emit heat into a ScalarField; Laplacian diffusion with
//!   decay; buoyancy into EdgeField; semi-Lagrangian advection;
//!   divergence-free projection (Jacobi Poisson); paper cells with
//!   T > threshold convert to fire.
//!
//! Measured axes:
//!   - throughput (ticks/sec) — raw speed
//!   - quality (propagation distance by tick T) — expressiveness proxy
//!     * M1: binary spread, no temperature field, no cooling
//!     * v3: temperature-gated, cooling, wind-driven anisotropy
//!
//! Criterion reports both in a single group; observe the cost of
//! expressiveness (v3 slower per tick) vs the cost of inflexibility
//! (M1 cannot represent wet paper, partial burn, airflow deflection).

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use kami_dec::*;

const AIR: u8 = 0;
const PAPER: u8 = 1;
const FIRE: u8 = 2;

fn build_fixture(n: i32) -> Vec<Vec<Vec<u8>>> {
    let size = n as usize;
    let mut grid = vec![vec![vec![AIR; size]; size]; size];
    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                grid[x][y][z] = PAPER;
            }
        }
    }
    let mid = size / 2;
    for dz in 0..3 {
        for dy in 0..3 {
            for dx in 0..3 {
                grid[mid - 1 + dx][mid - 1 + dy][mid - 1 + dz] = FIRE;
            }
        }
    }
    grid
}

// ── M1 rule-based tick ─────────────────────────────────────────────
fn tick_m1(grid: &mut Vec<Vec<Vec<u8>>>) {
    let n = grid.len();
    let mut ignite: Vec<(usize, usize, usize)> = Vec::new();
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                if grid[x][y][z] != PAPER {
                    continue;
                }
                let mut hot = false;
                for (dx, dy, dz) in [
                    (1i32, 0, 0),
                    (-1, 0, 0),
                    (0, 1, 0),
                    (0, -1, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                ] {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let nz = z as i32 + dz;
                    if nx < 0
                        || ny < 0
                        || nz < 0
                        || nx >= n as i32
                        || ny >= n as i32
                        || nz >= n as i32
                    {
                        continue;
                    }
                    if grid[nx as usize][ny as usize][nz as usize] == FIRE {
                        hot = true;
                        break;
                    }
                }
                if hot {
                    ignite.push((x, y, z));
                }
            }
        }
    }
    for (x, y, z) in ignite {
        grid[x][y][z] = FIRE;
    }
}

// ── v3 DEC tick ────────────────────────────────────────────────────
struct Ns {
    grid: Vec<Vec<Vec<u8>>>,
    heat: ScalarField,
    wind: EdgeField,
}

impl Ns {
    fn new(grid: Vec<Vec<Vec<u8>>>) -> Self {
        Self {
            grid,
            heat: ScalarField::new(),
            wind: EdgeField::new(),
        }
    }

    fn tick(&mut self, dt: f32) {
        let n = self.grid.len() as i32;
        // 1. Emit: fire → heat.
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    if self.grid[x as usize][y as usize][z as usize] == FIRE {
                        self.heat.add(x, y, z, 180.0 * dt);
                    }
                }
            }
        }
        // 2. Buoyancy + advection + projection.
        self.wind.damp(0.94);
        self.wind.add_buoyancy_from(&self.heat, 0.08, 1.0);
        project_divergence_free(&mut self.wind, 8);
        self.heat.advect_field(&self.wind, dt);
        // 3. Diffusion + decay.
        self.heat.diffuse(0.10, dt.min(0.05), 0.8);
        // 4. Ignite: paper with T > threshold → fire.
        let mut ignite: Vec<(usize, usize, usize)> = Vec::new();
        self.heat.for_each_nonzero(40.0, |x, y, z, _v| {
            if x < 0 || y < 0 || z < 0 || x >= n || y >= n || z >= n {
                return;
            }
            let (xu, yu, zu) = (x as usize, y as usize, z as usize);
            if self.grid[xu][yu][zu] == PAPER {
                ignite.push((xu, yu, zu));
            }
        });
        for (x, y, z) in ignite {
            self.grid[x][y][z] = FIRE;
        }
    }
}

fn count_fire(grid: &Vec<Vec<Vec<u8>>>) -> u32 {
    grid.iter()
        .flatten()
        .flatten()
        .filter(|&&v| v == FIRE)
        .count() as u32
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("fire-propagation");
    for &n in &[16_i32, 24, 32] {
        group.throughput(Throughput::Elements((n * n * n) as u64));
        group.bench_with_input(BenchmarkId::new("m1-rule", n), &n, |b, &n| {
            b.iter_batched(
                || build_fixture(n),
                |mut g| {
                    tick_m1(&mut g);
                    g
                },
                criterion::BatchSize::SmallInput,
            );
        });
        group.bench_with_input(BenchmarkId::new("v3-ns", n), &n, |b, &n| {
            b.iter_batched(
                || Ns::new(build_fixture(n)),
                |mut s| {
                    s.tick(1.0 / 60.0);
                    s
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// Quality snapshot printed during bench — fire count after 20 ticks.
fn quality_snapshot() {
    for &n in &[16_i32, 32] {
        let mut g = build_fixture(n);
        for _ in 0..20 {
            tick_m1(&mut g);
        }
        let m1 = count_fire(&g);
        let mut s = Ns::new(build_fixture(n));
        for _ in 0..20 {
            s.tick(1.0 / 60.0);
        }
        let ns = count_fire(&s.grid);
        eprintln!(
            "[quality] n={} after 20 ticks: m1={} fire cells, v3-ns={} fire cells",
            n, m1, ns
        );
    }
}

fn with_quality(c: &mut Criterion) {
    quality_snapshot();
    bench(c);
}

criterion_group!(benches, with_quality);
criterion_main!(benches);
