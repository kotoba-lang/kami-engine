//! GIS mesh generation: tile quads, globe patches, GeoJSON lines/polygons, ribbons.

use crate::projection::{LngLat, TileCoord, WorldPx, lng_lat_to_world_px};

/// Split a (lng, lat) ring into 1+ subrings whenever it crosses the
/// antimeridian (180°/-180°). Without this, polygons spanning the dateline
/// (Antarctica in Natural Earth ne_110m_land, Russia, Fiji, Aleutians, …)
/// are passed straight to earcut and produce one giant degenerate triangle
/// that wraps the whole world (visible as the "fragmented globe" bug,
/// 2026-05-05).
///
/// Algorithm: walk the ring; when consecutive longitudes differ by > 180°,
/// shift one of them by ±360° and emit a synthetic break-point at ±180°.
/// The result is a list of OGC simple-features compliant subrings, each
/// of which lies entirely on one side of the dateline (so earcut + flat /
/// globe projection both behave).
///
/// Open input ring (no duplicated closing vertex) is acceptable; we close
/// each output subring before returning.
pub fn split_antimeridian(ring: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
    if ring.len() < 3 {
        return vec![ring.to_vec()];
    }
    // Find dateline crossings.
    let mut subrings: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut current: Vec<[f64; 2]> = Vec::with_capacity(ring.len());
    current.push(ring[0]);
    for i in 1..ring.len() {
        let prev = ring[i - 1];
        let curr = ring[i];
        let dlon = curr[0] - prev[0];
        if dlon > 180.0 || dlon < -180.0 {
            // Crossing detected. Compute lat at the dateline by linear
            // interpolation in *unwrapped* lon space.
            let (prev_unwrap, curr_unwrap) = if dlon > 180.0 {
                // curr jumped east → west crossing (e.g. -179 → +179).
                ([prev[0] + 360.0, prev[1]], curr)
            } else {
                ([prev[0] - 360.0, prev[1]], curr)
            };
            // Find the dateline (±180° in the unwrapped frame).
            let target_lon = if dlon > 180.0 { 180.0 } else { -180.0 };
            // t = fraction from prev_unwrap to curr_unwrap where lon = ±180.
            let span = curr_unwrap[0] - prev_unwrap[0];
            let t = if span.abs() < 1e-12 {
                0.5
            } else {
                (target_lon - prev_unwrap[0]) / span
            };
            let break_lat = prev_unwrap[1] + t * (curr_unwrap[1] - prev_unwrap[1]);
            // Close the current subring at one side of the dateline,
            // start a new subring at the other side.
            let cap_lon_a = if dlon > 180.0 { -180.0 } else { 180.0 };
            let cap_lon_b = if dlon > 180.0 { 180.0 } else { -180.0 };
            current.push([cap_lon_a, break_lat]);
            subrings.push(std::mem::take(&mut current));
            current.push([cap_lon_b, break_lat]);
            current.push(curr);
        } else {
            current.push(curr);
        }
    }
    if !current.is_empty() {
        subrings.push(current);
    }
    // Close each subring (earcut tolerates open or closed; explicit close is
    // friendlier to downstream extrude code which dedups duplicates).
    for sub in subrings.iter_mut() {
        if sub.len() >= 2 && sub[0] != sub[sub.len() - 1] {
            sub.push(sub[0]);
        }
    }
    if subrings.is_empty() {
        return vec![ring.to_vec()];
    }
    subrings
}

#[cfg(test)]
mod antimeridian_tests {
    use super::split_antimeridian;

    #[test]
    fn no_crossing_passes_through() {
        let ring = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let out = split_antimeridian(&ring);
        assert_eq!(out.len(), 1, "non-crossing ring should be 1 subring");
    }

    #[test]
    fn dateline_crossing_splits_into_two() {
        // Fiji-style ring straddling +180 / -180.
        let ring = vec![
            [179.0, -10.0],
            [-179.0, -10.0],
            [-179.0, 10.0],
            [179.0, 10.0],
        ];
        let out = split_antimeridian(&ring);
        assert!(out.len() >= 2, "dateline-crossing ring must split");
        for sub in &out {
            for p in sub {
                assert!(
                    p[0] >= -180.0 - 1e-6 && p[0] <= 180.0 + 1e-6,
                    "subring lon {} out of range",
                    p[0]
                );
            }
        }
    }

    #[test]
    fn antarctica_like_long_strip() {
        // Simplified Antarctica outline: long east-to-west strip across dateline.
        let ring = vec![
            [-170.0, -60.0],
            [170.0, -60.0],
            [170.0, -85.0],
            [-170.0, -85.0],
        ];
        let out = split_antimeridian(&ring);
        // Should detect at least one crossing.
        assert!(
            out.len() >= 2,
            "Antarctica-like strip must split at dateline (got {} subrings)",
            out.len()
        );
    }
}

/// Interleaved vertex: pos3 + norm3 + uv2 = 8 floats = 32 bytes.
/// Compatible with kami-render's `vertex_buffer_layout()`.
pub struct GeoMesh {
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
}

/// Generate a textured quad for a single raster tile.
/// The quad is 256×256 world-pixels, placed at (0,0,0).
/// Caller applies a transform to position it correctly.
/// Normal points up (+Y).  UV maps the full tile texture.
pub fn tile_quad() -> GeoMesh {
    let s = 256.0_f32;
    // 4 vertices: top-left, top-right, bottom-right, bottom-left
    // World: X = east, Z = south, Y = up
    // Winding order: CCW when viewed from above (+Y), so normal points UP.
    // Vertices ordered: TL → BL → BR → TR (CCW from top view).
    #[rustfmt::skip]
    let vertices = vec![
        // pos                norm          uv
        0.0, 0.0, 0.0,      0.0, 1.0, 0.0,   0.0, 0.0,  // 0: TL
        0.0, 0.0, s,        0.0, 1.0, 0.0,   0.0, 1.0,  // 1: BL
        s,   0.0, s,        0.0, 1.0, 0.0,   1.0, 1.0,  // 2: BR
        s,   0.0, 0.0,      0.0, 1.0, 0.0,   1.0, 0.0,  // 3: TR
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    GeoMesh { vertices, indices }
}

/// Generate a textured flat Web-Mercator tile patch displaced by DEM heights.
///
/// Positions are local tile pixels (0..256 in X/Z) so callers can keep using
/// the same per-tile transform as `tile_quad()`. Height is converted from
/// meters to world pixels at the tile latitude and zoom.
pub fn flat_tile_patch_from_dem(
    coord: TileCoord,
    segments: u32,
    heights_m: &[f32],
    dem_width: u32,
    dem_height: u32,
    vertical_exaggeration: f32,
) -> GeoMesh {
    if dem_width < 2 || dem_height < 2 || heights_m.len() < (dem_width * dem_height) as usize {
        return tile_quad();
    }

    let (_, south, _, north) = coord.lng_lat_bounds();
    let mid_lat = ((south + north) * 0.5).to_radians();
    let meters_per_px =
        (156_543.033_92_f64 * mid_lat.cos() / 2.0_f64.powi(coord.z as i32)).max(0.05) as f32;
    let meters_to_world_px = vertical_exaggeration.max(0.0) / meters_per_px;
    let segs = segments.clamp(2, 64);
    let stride = segs + 1;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity((stride * stride) as usize);
    let mut normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; (stride * stride) as usize];
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity((stride * stride) as usize);
    let mut indices = Vec::with_capacity((segs * segs * 6) as usize);

    for iy in 0..=segs {
        let v = iy as f32 / segs as f32;
        for ix in 0..=segs {
            let u = ix as f32 / segs as f32;
            let height = sample_dem_height(heights_m, dem_width, dem_height, u, v)
                .mul_add(meters_to_world_px, 0.0)
                .clamp(-4096.0, 4096.0);
            positions.push([u * 256.0, height, v * 256.0]);
            uvs.push([u, v]);
        }
    }

    for iy in 0..segs {
        for ix in 0..segs {
            let a = iy * stride + ix;
            let b = a + stride;
            indices.extend_from_slice(&[a, b, b + 1, a, b + 1, a + 1]);
        }
    }

    for tri in indices.chunks_exact(3) {
        let ia = tri[0] as usize;
        let ib = tri[1] as usize;
        let ic = tri[2] as usize;
        let ab = sub3(positions[ib], positions[ia]);
        let ac = sub3(positions[ic], positions[ia]);
        let face = normalize3(cross3(ab, ac));
        normals[ia] = add3(normals[ia], face);
        normals[ib] = add3(normals[ib], face);
        normals[ic] = add3(normals[ic], face);
    }

    let mut vertices = Vec::with_capacity((stride * stride * 8) as usize);
    for (idx, pos) in positions.iter().enumerate() {
        let normal = normalize3(normals[idx]);
        let uv = uvs[idx];
        vertices.extend_from_slice(&[
            pos[0], pos[1], pos[2], normal[0], normal[1], normal[2], uv[0], uv[1],
        ]);
    }

    GeoMesh { vertices, indices }
}

/// Generate a sphere patch for a raster tile.
/// The patch is centered on the origin and textured with the full tile image.
pub fn globe_tile_patch(coord: TileCoord, radius: f32, segments: u32) -> GeoMesh {
    globe_tile_patch_terrain(coord, radius, segments, radius * 0.028)
}

/// Generate a sphere patch with procedural relief suitable for a globe terrain view.
pub fn globe_tile_patch_terrain(
    coord: TileCoord,
    radius: f32,
    segments: u32,
    terrain_scale: f32,
) -> GeoMesh {
    globe_tile_patch_from_heights(coord, radius, segments, terrain_scale, None, 0, 0)
}

/// Generate a sphere patch from a DEM height tile. Heights are interpreted in meters.
pub fn globe_tile_patch_from_dem(
    coord: TileCoord,
    radius: f32,
    segments: u32,
    heights_m: &[f32],
    dem_width: u32,
    dem_height: u32,
    meters_to_radius: f32,
) -> GeoMesh {
    globe_tile_patch_from_heights(
        coord,
        radius,
        segments,
        meters_to_radius,
        Some(heights_m),
        dem_width,
        dem_height,
    )
}

fn globe_tile_patch_from_heights(
    coord: TileCoord,
    radius: f32,
    segments: u32,
    height_scale: f32,
    dem_heights: Option<&[f32]>,
    dem_width: u32,
    dem_height: u32,
) -> GeoMesh {
    let (west, south, east, north) = coord.lng_lat_bounds();
    let segs = segments.max(2);
    let stride = segs + 1;
    let mut vertices = Vec::with_capacity((stride * stride * 8) as usize);
    let mut indices = Vec::with_capacity((segs * segs * 6) as usize);
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity((stride * stride) as usize);
    let mut normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; (stride * stride) as usize];
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity((stride * stride) as usize);

    for iy in 0..=segs {
        let v = iy as f64 / segs as f64;
        let lat = north + (south - north) * v;
        for ix in 0..=segs {
            let u = ix as f64 / segs as f64;
            let lng = west + (east - west) * u;
            let height = if let Some(heights) = dem_heights {
                sample_dem_height(heights, dem_width, dem_height, u as f32, v as f32) * height_scale
            } else {
                globe_relief_height(lng, lat) * height_scale
            };
            let (x, y, z) = lng_lat_to_sphere_xyz(lng, lat, radius + height);
            positions.push([x, y, z]);
            uvs.push([u as f32, v as f32]);
        }
    }

    for iy in 0..segs {
        for ix in 0..segs {
            let a = iy * stride + ix;
            let b = a + stride;
            indices.extend_from_slice(&[a, b + 1, b, a, a + 1, b + 1]);
        }
    }

    for tri in indices.chunks_exact(3) {
        let ia = tri[0] as usize;
        let ib = tri[1] as usize;
        let ic = tri[2] as usize;
        let ab = sub3(positions[ib], positions[ia]);
        let ac = sub3(positions[ic], positions[ia]);
        let face = normalize3(cross3(ab, ac));
        normals[ia] = add3(normals[ia], face);
        normals[ib] = add3(normals[ib], face);
        normals[ic] = add3(normals[ic], face);
    }

    for (idx, pos) in positions.iter().enumerate() {
        let normal = normalize3(normals[idx]);
        let uv = uvs[idx];
        vertices.extend_from_slice(&[
            pos[0], pos[1], pos[2], normal[0], normal[1], normal[2], uv[0], uv[1],
        ]);
    }

    GeoMesh { vertices, indices }
}

fn sample_dem_height(heights: &[f32], width: u32, height: u32, u: f32, v: f32) -> f32 {
    if width < 2 || height < 2 || heights.len() < (width * height) as usize {
        return 0.0;
    }
    let x = u.clamp(0.0, 1.0) * (width - 1) as f32;
    let y = v.clamp(0.0, 1.0) * (height - 1) as f32;
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width as usize - 1);
    let y1 = (y0 + 1).min(height as usize - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let idx = |xx: usize, yy: usize| yy * width as usize + xx;
    let h00 = heights[idx(x0, y0)];
    let h10 = heights[idx(x1, y0)];
    let h01 = heights[idx(x0, y1)];
    let h11 = heights[idx(x1, y1)];
    let hx0 = lerp(h00, h10, tx);
    let hx1 = lerp(h01, h11, tx);
    lerp(hx0, hx1, ty)
}

fn globe_relief_height(lng: f64, lat: f64) -> f32 {
    let x = lng as f32 / 180.0;
    let y = lat as f32 / 90.0;
    let lat_band = 1.0 - (lat.abs() as f32 / 90.0).powf(1.35);
    let continent = fbm2(x * 1.15 + 13.7, y * 1.15 - 5.4, 4, 2.0, 0.52);
    let shelf = fbm2(x * 2.6 - 8.1, y * 2.6 + 4.7, 3, 2.1, 0.5);
    let land_mask = smoothstep(
        -0.08,
        0.22,
        continent * 0.9 + shelf * 0.35 + lat_band * 0.14,
    );
    let ridges = ridged_fbm2(x * 6.5 + 1.9, y * 6.5 - 3.7, 5, 2.05, 0.56);
    let hills = fbm2(x * 9.0 - 12.3, y * 9.0 + 2.8, 4, 2.0, 0.5);
    let ocean = fbm2(x * 4.2 + 6.6, y * 4.2 - 1.1, 3, 2.2, 0.45);
    let mountains = land_mask.powf(2.2) * ridges.powf(1.7);
    let uplands = land_mask * (0.18 + hills * 0.28);
    let ocean_depth = -(1.0 - land_mask) * (0.12 + 0.08 * ocean);
    uplands + mountains * 0.72 + ocean_depth
}

fn fbm2(mut x: f32, mut y: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut amp = 0.5;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for _ in 0..octaves {
        sum += amp * value_noise2(x, y);
        norm += amp;
        x *= lacunarity;
        y *= lacunarity;
        amp *= gain;
    }
    if norm > 0.0 { sum / norm } else { 0.0 }
}

fn ridged_fbm2(mut x: f32, mut y: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut amp = 0.5;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for _ in 0..octaves {
        let n = 1.0 - (value_noise2(x, y) * 2.0 - 1.0).abs();
        sum += amp * n * n;
        norm += amp;
        x *= lacunarity;
        y *= lacunarity;
        amp *= gain;
    }
    if norm > 0.0 { sum / norm } else { 0.0 }
}

fn value_noise2(x: f32, y: f32) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let tx = x - x0;
    let ty = y - y0;
    let sx = smoothstep(0.0, 1.0, tx);
    let sy = smoothstep(0.0, 1.0, ty);
    let n00 = hash2(x0 as i32, y0 as i32);
    let n10 = hash2(x0 as i32 + 1, y0 as i32);
    let n01 = hash2(x0 as i32, y0 as i32 + 1);
    let n11 = hash2(x0 as i32 + 1, y0 as i32 + 1);
    let nx0 = lerp(n00, n10, sx);
    let nx1 = lerp(n01, n11, sx);
    lerp(nx0, nx1, sy)
}

fn hash2(x: i32, y: i32) -> f32 {
    let mut n = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) as f32) / u32::MAX as f32
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lng_lat_to_sphere_xyz(lng: f64, lat: f64, radius: f32) -> (f32, f32, f32) {
    let lng_rad = lng.to_radians();
    let lat_rad = lat.to_radians();
    let cos_lat = lat_rad.cos() as f32;
    let sin_lat = lat_rad.sin() as f32;
    let sin_lng = lng_rad.sin() as f32;
    let cos_lng = lng_rad.cos() as f32;
    (
        radius * cos_lat * sin_lng,
        radius * sin_lat,
        -radius * cos_lat * cos_lng,
    )
}

/// Generate a ribbon following the globe surface.
pub fn globe_line_to_ribbon(
    coords_lng_lat: &[[f64; 2]],
    radius: f32,
    width_world: f32,
    elevation: f32,
) -> GeoMesh {
    if coords_lng_lat.len() < 2 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let positions: Vec<[f32; 3]> = coords_lng_lat
        .iter()
        .map(|c| sphere_position(c[0], c[1], radius + elevation))
        .collect();
    let normals: Vec<[f32; 3]> = coords_lng_lat
        .iter()
        .map(|c| sphere_normal(c[0], c[1]))
        .collect();

    let mut vertices = Vec::with_capacity(coords_lng_lat.len() * 2 * 8);
    let mut indices = Vec::with_capacity((coords_lng_lat.len() - 1) * 6);
    let half_w = width_world * 0.5;

    for i in 0..coords_lng_lat.len() {
        let tangent = segment_tangent(&positions, i);
        let normal = normals[i];
        let binormal = normalize3(cross3(normal, tangent));
        let left = add3(positions[i], mul3(binormal, half_w));
        let right = add3(positions[i], mul3(binormal, -half_w));
        let u = i as f32 / (coords_lng_lat.len() - 1) as f32;

        vertices.extend_from_slice(&[
            left[0], left[1], left[2], normal[0], normal[1], normal[2], u, 0.0,
        ]);
        vertices.extend_from_slice(&[
            right[0], right[1], right[2], normal[0], normal[1], normal[2], u, 1.0,
        ]);
    }

    for i in 0..(coords_lng_lat.len() - 1) as u32 {
        let base = i * 2;
        indices.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    }

    GeoMesh { vertices, indices }
}

/// Generate a polygon fill draped on the globe surface.
///
/// Phase A antimeridian fix (2026-05-06): split at ±180° crossings before
/// triangulation. Without this, dateline-spanning rings (Antarctica in
/// Natural Earth) get earcut'd in a frame where some vertices are unwrapped
/// to lon = +540° and others stay at -170°, generating one huge wrong
/// triangle that wraps the globe.
pub fn globe_polygon_to_fill_earcut(
    ring_lng_lat: &[[f64; 2]],
    radius: f32,
    elevation: f32,
) -> GeoMesh {
    if ring_lng_lat.len() < 3 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let subrings = split_antimeridian(ring_lng_lat);
    if subrings.len() > 1 {
        let mut combined = GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        for sub in subrings {
            let mesh = globe_polygon_to_fill_earcut_simple(&sub, radius, elevation);
            let base = (combined.vertices.len() / 8) as u32;
            combined.vertices.extend(mesh.vertices);
            combined
                .indices
                .extend(mesh.indices.into_iter().map(|i| i + base));
        }
        return combined;
    }

    globe_polygon_to_fill_earcut_simple(ring_lng_lat, radius, elevation)
}

/// Inner globe-fill path (no antimeridian handling).
fn globe_polygon_to_fill_earcut_simple(
    ring_lng_lat: &[[f64; 2]],
    radius: f32,
    elevation: f32,
) -> GeoMesh {
    let mut coords: Vec<[f64; 2]> = ring_lng_lat.to_vec();
    if coords.len() >= 2 && coords.first() == coords.last() {
        coords.pop();
    }
    if coords.len() < 3 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let anchor_lng = coords[0][0];
    let points_2d: Vec<[f32; 2]> = coords
        .iter()
        .map(|c| [unwrap_lng(c[0], anchor_lng) as f32, c[1] as f32])
        .collect();
    let mut vertices = Vec::with_capacity(coords.len() * 8);
    for c in &coords {
        let pos = sphere_position(c[0], c[1], radius + elevation);
        let n = sphere_normal(c[0], c[1]);
        vertices.extend_from_slice(&[pos[0], pos[1], pos[2], n[0], n[1], n[2], 0.5, 0.5]);
    }
    let indices = earcut_indices(&points_2d);
    GeoMesh { vertices, indices }
}

/// Generate circle discs tangent to the globe surface.
pub fn globe_points_to_circles(
    points_lng_lat: &[[f64; 2]],
    radius: f32,
    disc_radius: f32,
    elevation: f32,
    segments: u32,
) -> GeoMesh {
    if points_lng_lat.is_empty() {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }
    let seg = segments.max(6) as usize;
    let mut vertices = Vec::with_capacity(points_lng_lat.len() * (seg + 1) * 8);
    let mut indices = Vec::with_capacity(points_lng_lat.len() * seg * 3);

    for p in points_lng_lat {
        let center = sphere_position(p[0], p[1], radius + elevation);
        let normal = sphere_normal(p[0], p[1]);
        let east_seed = if normal[1].abs() > 0.98 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let east = normalize3(cross3(east_seed, normal));
        let north = normalize3(cross3(normal, east));
        let base = (vertices.len() / 8) as u32;

        vertices.extend_from_slice(&[
            center[0], center[1], center[2], normal[0], normal[1], normal[2], 0.5, 0.5,
        ]);
        for i in 0..seg {
            let theta = (i as f32 / seg as f32) * std::f32::consts::TAU;
            let rim = add3(
                center,
                add3(
                    mul3(east, theta.cos() * disc_radius),
                    mul3(north, theta.sin() * disc_radius),
                ),
            );
            vertices.extend_from_slice(&[
                rim[0],
                rim[1],
                rim[2],
                normal[0],
                normal[1],
                normal[2],
                0.5 + 0.5 * theta.cos(),
                0.5 + 0.5 * theta.sin(),
            ]);
        }
        for i in 0..seg as u32 {
            indices.push(base);
            indices.push(base + 1 + i);
            indices.push(base + 1 + ((i + 1) % seg as u32));
        }
    }

    GeoMesh { vertices, indices }
}

/// Generate a ribbon (flat strip with width) from a polyline.
/// Coordinates are in world-pixel space relative to center_px.
/// The ribbon lies on the Y=elevation plane.
pub fn line_to_ribbon(
    coords_lng_lat: &[[f64; 2]],
    zoom: f64,
    center_px: WorldPx,
    width: f32,
    elevation: f32,
) -> GeoMesh {
    if coords_lng_lat.len() < 2 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let points: Vec<[f32; 2]> = coords_lng_lat
        .iter()
        .map(|c| {
            let wp = lng_lat_to_world_px(LngLat::new(c[0], c[1]), zoom);
            [(wp.x - center_px.x) as f32, (wp.y - center_px.y) as f32]
        })
        .collect();

    let half_w = width * 0.5;
    let n = points.len();
    let mut vertices = Vec::with_capacity(n * 2 * 8);
    let mut indices = Vec::with_capacity((n - 1) * 6);

    for i in 0..n {
        // Compute perpendicular direction
        let (dx, dz) = if i == 0 {
            (points[1][0] - points[0][0], points[1][1] - points[0][1])
        } else if i == n - 1 {
            (
                points[n - 1][0] - points[n - 2][0],
                points[n - 1][1] - points[n - 2][1],
            )
        } else {
            (
                points[i + 1][0] - points[i - 1][0],
                points[i + 1][1] - points[i - 1][1],
            )
        };
        let len = (dx * dx + dz * dz).sqrt().max(1e-6);
        let nx = -dz / len;
        let nz = dx / len;

        let px = points[i][0];
        let pz = points[i][1];
        let u = i as f32 / (n - 1) as f32;

        // Left vertex
        vertices.extend_from_slice(&[
            px + nx * half_w,
            elevation,
            pz + nz * half_w,
            0.0,
            1.0,
            0.0,
            u,
            0.0,
        ]);
        // Right vertex
        vertices.extend_from_slice(&[
            px - nx * half_w,
            elevation,
            pz - nz * half_w,
            0.0,
            1.0,
            0.0,
            u,
            1.0,
        ]);
    }

    for i in 0..(n - 1) as u32 {
        let base = i * 2;
        indices.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
    }

    GeoMesh { vertices, indices }
}

/// Generate a flat polygon fill from a ring of coordinates.
/// Uses a simple ear-clipping triangulation for convex-ish polygons.
pub fn polygon_to_fill(
    ring_lng_lat: &[[f64; 2]],
    zoom: f64,
    center_px: WorldPx,
    elevation: f32,
) -> GeoMesh {
    if ring_lng_lat.len() < 3 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let points: Vec<[f32; 2]> = ring_lng_lat
        .iter()
        .map(|c| {
            let wp = lng_lat_to_world_px(LngLat::new(c[0], c[1]), zoom);
            [(wp.x - center_px.x) as f32, (wp.y - center_px.y) as f32]
        })
        .collect();

    let n = points.len();
    let mut vertices = Vec::with_capacity(n * 8);
    for p in points.iter() {
        let u = p[0] / 256.0;
        let v = p[1] / 256.0;
        vertices.extend_from_slice(&[p[0], elevation, p[1], 0.0, 1.0, 0.0, u, v]);
    }

    // Fan triangulation from vertex 0 (works for convex polygons)
    let mut indices = Vec::with_capacity((n - 2) * 3);
    for i in 1..(n - 1) {
        indices.push(0);
        indices.push(i as u32);
        indices.push((i + 1) as u32);
    }

    GeoMesh { vertices, indices }
}

/// Ear-clipping triangulation for simple (possibly concave) polygons without holes.
/// Input ring is treated as 2D XZ points (Y is elevation). No self-intersection support.
///
/// Phase A antimeridian fix (2026-05-06): if the ring crosses ±180°, it is
/// split into 1+ subrings via `split_antimeridian()` and each subring is
/// triangulated independently then concatenated. Without this, dateline-
/// crossing polygons (Antarctica / Russia / Fiji) produce a single huge
/// degenerate triangle that wraps the world (the "fragmented globe" bug).
pub fn polygon_to_fill_earcut(
    ring_lng_lat: &[[f64; 2]],
    zoom: f64,
    center_px: WorldPx,
    elevation: f32,
) -> GeoMesh {
    if ring_lng_lat.len() < 3 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    // Phase A: split at antimeridian (±180°) crossings, then triangulate
    // each subring independently. Fast-path single-subring case.
    let subrings = split_antimeridian(ring_lng_lat);
    if subrings.len() > 1 {
        let mut combined = GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        for sub in subrings {
            let mesh = polygon_to_fill_earcut_simple(&sub, zoom, center_px, elevation);
            let base = (combined.vertices.len() / 8) as u32;
            combined.vertices.extend(mesh.vertices);
            combined
                .indices
                .extend(mesh.indices.into_iter().map(|i| i + base));
        }
        return combined;
    }

    polygon_to_fill_earcut_simple(ring_lng_lat, zoom, center_px, elevation)
}

/// Inner simple-polygon path (no antimeridian handling) — extracted from
/// the original body so the public `polygon_to_fill_earcut` can wrap it
/// per-subring after the antimeridian split.
fn polygon_to_fill_earcut_simple(
    ring_lng_lat: &[[f64; 2]],
    zoom: f64,
    center_px: WorldPx,
    elevation: f32,
) -> GeoMesh {
    if ring_lng_lat.len() < 3 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let mut points: Vec<[f32; 2]> = ring_lng_lat
        .iter()
        .map(|c| {
            let wp = lng_lat_to_world_px(LngLat::new(c[0], c[1]), zoom);
            [(wp.x - center_px.x) as f32, (wp.y - center_px.y) as f32]
        })
        .collect();

    // Drop duplicated closing vertex if present.
    if points.len() >= 2 && points[0] == points[points.len() - 1] {
        points.pop();
    }
    let n = points.len();
    if n < 3 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let mut vertices = Vec::with_capacity(n * 8);
    for p in points.iter() {
        let u = p[0] / 256.0;
        let v = p[1] / 256.0;
        vertices.extend_from_slice(&[p[0], elevation, p[1], 0.0, 1.0, 0.0, u, v]);
    }

    // Determine winding. Ear-clipping assumes CCW in XZ (as viewed from +Y down).
    let signed_area: f32 = (0..n)
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % n];
            (b[0] - a[0]) * (b[1] + a[1])
        })
        .sum::<f32>()
        * 0.5;
    let mut ring: Vec<usize> = if signed_area < 0.0 {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };

    let mut indices = Vec::with_capacity((n - 2) * 3);
    let mut guard = ring.len() * ring.len();
    while ring.len() >= 3 && guard > 0 {
        guard -= 1;
        let mut clipped = false;
        for i in 0..ring.len() {
            let ia = ring[(i + ring.len() - 1) % ring.len()];
            let ib = ring[i];
            let ic = ring[(i + 1) % ring.len()];
            let a = points[ia];
            let b = points[ib];
            let c = points[ic];
            // Convex test (CCW).
            // f64 to keep the sign stable when world-px coords exceed ~1e6.
            let cross = convex_cross_f64(a, b, c);
            if cross <= 0.0 {
                continue;
            }
            // Ensure no other vertex lies inside the triangle.
            let mut contains = false;
            for &j in ring.iter() {
                if j == ia || j == ib || j == ic {
                    continue;
                }
                let p = points[j];
                if point_in_triangle(p, a, b, c) {
                    contains = true;
                    break;
                }
            }
            if contains {
                continue;
            }
            indices.push(ia as u32);
            indices.push(ib as u32);
            indices.push(ic as u32);
            ring.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            // Degenerate / self-intersecting; fall back to fan on remainder.
            let first = ring[0] as u32;
            for w in ring.windows(2).skip(1) {
                indices.push(first);
                indices.push(w[0] as u32);
                indices.push(w[1] as u32);
            }
            break;
        }
    }

    // Earcut produces CCW-in-2D triangles. In the XZ ground plane (2D X→3D X,
    // 2D Y→3D Z), the 3D surface normal = -Y (down), so wgpu back-face culls
    // them under FrontFace::Ccw. Swap i1↔i2 per triangle to get N.y > 0.
    for tri in indices.chunks_exact_mut(3) {
        tri.swap(1, 2);
    }

    GeoMesh { vertices, indices }
}

fn point_in_triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let s1 = sign(p, a, b);
    let s2 = sign(p, b, c);
    let s3 = sign(p, c, a);
    let has_neg = s1 < 0.0 || s2 < 0.0 || s3 < 0.0;
    let has_pos = s1 > 0.0 || s2 > 0.0 || s3 > 0.0;
    !(has_neg && has_pos)
}

// Compute signed area / cross product in f64 and return f32 sign. f32 loses
// ~6 significant digits on products of world-px coords at zoom ≥ 7 (Tokyo =
// 1e6 px → cross ~1e12, sign unreliable). f64 has 15-16 digits → stable up to
// planet-scale polygons.
fn sign(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let r = (p[0] as f64 - b[0] as f64) * (a[1] as f64 - b[1] as f64)
        - (a[0] as f64 - b[0] as f64) * (p[1] as f64 - b[1] as f64);
    r as f32
}

/// f64-precision convex-corner test for the earcut ear-clipping inner loop.
/// Returns the signed cross product of the edges (a→b) and (a→c). f32 input
/// is widened to f64 only inside the multiply so the numerator keeps its sign
/// at world-px scale.
#[inline]
fn convex_cross_f64(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f64 {
    (b[0] as f64 - a[0] as f64) * (c[1] as f64 - a[1] as f64)
        - (b[1] as f64 - a[1] as f64) * (c[0] as f64 - a[0] as f64)
}

fn sphere_position(lng: f64, lat: f64, radius: f32) -> [f32; 3] {
    let (x, y, z) = lng_lat_to_sphere_xyz(lng, lat, radius);
    [x, y, z]
}

fn sphere_normal(lng: f64, lat: f64) -> [f32; 3] {
    let p = sphere_position(lng, lat, 1.0);
    normalize3(p)
}

fn segment_tangent(points: &[[f32; 3]], i: usize) -> [f32; 3] {
    let delta = if i == 0 {
        sub3(points[1], points[0])
    } else if i == points.len() - 1 {
        sub3(points[i], points[i - 1])
    } else {
        add3(
            sub3(points[i + 1], points[i]),
            sub3(points[i], points[i - 1]),
        )
    };
    normalize3(delta)
}

fn earcut_indices(points: &[[f32; 2]]) -> Vec<u32> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let signed_area: f32 = (0..n)
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % n];
            (b[0] - a[0]) * (b[1] + a[1])
        })
        .sum::<f32>()
        * 0.5;
    let mut ring: Vec<usize> = if signed_area < 0.0 {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };
    let mut indices = Vec::with_capacity((n - 2) * 3);
    let mut guard = ring.len() * ring.len();
    while ring.len() >= 3 && guard > 0 {
        guard -= 1;
        let mut clipped = false;
        for i in 0..ring.len() {
            let ia = ring[(i + ring.len() - 1) % ring.len()];
            let ib = ring[i];
            let ic = ring[(i + 1) % ring.len()];
            let a = points[ia];
            let b = points[ib];
            let c = points[ic];
            // f64 to keep the sign stable when world-px coords exceed ~1e6.
            let cross = convex_cross_f64(a, b, c);
            if cross <= 0.0 {
                continue;
            }
            let mut contains = false;
            for &j in &ring {
                if j == ia || j == ib || j == ic {
                    continue;
                }
                if point_in_triangle(points[j], a, b, c) {
                    contains = true;
                    break;
                }
            }
            if contains {
                continue;
            }
            indices.extend_from_slice(&[ia as u32, ib as u32, ic as u32]);
            ring.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            let first = ring[0] as u32;
            for w in ring.windows(2).skip(1) {
                indices.extend_from_slice(&[first, w[0] as u32, w[1] as u32]);
            }
            break;
        }
    }
    indices
}

fn unwrap_lng(lng: f64, anchor: f64) -> f64 {
    let mut out = lng;
    while out - anchor > 180.0 {
        out -= 360.0;
    }
    while out - anchor < -180.0 {
        out += 360.0;
    }
    out
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul3(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Generate batched circle discs for a set of point features.
/// `radius_world_px` is measured in world pixels at the layer creation zoom.
/// `segments` controls tessellation resolution (min 6).
pub fn points_to_circles(
    points_lng_lat: &[[f64; 2]],
    zoom: f64,
    center_px: WorldPx,
    radius_world_px: f32,
    elevation: f32,
    segments: u32,
) -> GeoMesh {
    if points_lng_lat.is_empty() {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }
    let seg = segments.max(6) as usize;
    let mut vertices = Vec::with_capacity(points_lng_lat.len() * (seg + 1) * 8);
    let mut indices = Vec::with_capacity(points_lng_lat.len() * seg * 3);

    for p in points_lng_lat {
        let wp = lng_lat_to_world_px(LngLat::new(p[0], p[1]), zoom);
        let cx = (wp.x - center_px.x) as f32;
        let cz = (wp.y - center_px.y) as f32;
        let base = (vertices.len() / 8) as u32;

        // Center vertex.
        vertices.extend_from_slice(&[cx, elevation, cz, 0.0, 1.0, 0.0, 0.5, 0.5]);
        for i in 0..seg {
            let theta = (i as f32 / seg as f32) * std::f32::consts::TAU;
            let dx = theta.cos() * radius_world_px;
            let dz = theta.sin() * radius_world_px;
            let u = 0.5 + 0.5 * theta.cos();
            let v = 0.5 + 0.5 * theta.sin();
            vertices.extend_from_slice(&[cx + dx, elevation, cz + dz, 0.0, 1.0, 0.0, u, v]);
        }
        for i in 0..seg as u32 {
            indices.push(base);
            indices.push(base + 1 + i);
            indices.push(base + 1 + ((i + 1) % seg as u32));
        }
    }

    GeoMesh { vertices, indices }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_quad_mesh() {
        let m = tile_quad();
        assert_eq!(m.vertices.len(), 4 * 8); // 4 verts × 8 floats
        assert_eq!(m.indices.len(), 6);
    }

    #[test]
    fn ribbon_two_points() {
        let center = WorldPx { x: 0.0, y: 0.0 };
        let coords = vec![[0.0, 0.0], [1.0, 0.0]];
        let m = line_to_ribbon(&coords, 0.0, center, 10.0, 0.0);
        assert_eq!(m.vertices.len(), 4 * 8); // 2 points × 2 sides × 8 floats
        assert_eq!(m.indices.len(), 6);
    }

    #[test]
    fn polygon_triangle() {
        let center = WorldPx { x: 0.0, y: 0.0 };
        let coords = vec![[0.0, 0.0], [10.0, 0.0], [5.0, 10.0]];
        let m = polygon_to_fill(&coords, 0.0, center, 0.0);
        assert_eq!(m.vertices.len(), 3 * 8);
        assert_eq!(m.indices.len(), 3);
    }

    #[test]
    fn polygon_earcut_concave() {
        let center = WorldPx { x: 0.0, y: 0.0 };
        // Concave arrow-like shape.
        let coords = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [5.0, 5.0],
            [10.0, 10.0],
            [0.0, 10.0],
        ];
        let m = polygon_to_fill_earcut(&coords, 0.0, center, 0.0);
        assert_eq!(m.vertices.len(), 5 * 8);
        // 5-gon -> 3 triangles -> 9 indices.
        assert_eq!(m.indices.len(), 9);
    }

    #[test]
    fn circles_batched() {
        let center = WorldPx { x: 0.0, y: 0.0 };
        let pts = vec![[0.0, 0.0], [1.0, 1.0]];
        let m = points_to_circles(&pts, 0.0, center, 4.0, 0.0, 8);
        // 2 points × (1 center + 8 rim) × 8 floats.
        assert_eq!(m.vertices.len(), 2 * 9 * 8);
        // 2 points × 8 triangles × 3 idx.
        assert_eq!(m.indices.len(), 2 * 8 * 3);
    }

    #[test]
    fn extrude_square_has_roof_and_walls() {
        let center = WorldPx { x: 0.0, y: 0.0 };
        let ring = vec![[0.0, 0.0], [0.001, 0.0], [0.001, 0.001], [0.0, 0.001]];
        let m = polygon_to_extrude_earcut(&ring, 0.0, center, 0.0, 10.0);
        // Roof = 4 verts, walls = 4 edges × 4 verts = 16. Total 20 verts × 8 floats.
        assert_eq!(m.vertices.len(), 20 * 8);
        // Roof 2 tri × 3 + Walls 4 edges × 2 tri × 3 = 6 + 24 = 30 indices.
        assert_eq!(m.indices.len(), 30);
    }
}

/// Extrude a polygon footprint (in lng/lat) upward by `height` world units.
/// Emits: a roof capped at y=base+height (triangulated via ear-clipping) plus
/// sidewall quads (2 triangles per edge) from y=base to y=base+height.
/// Sidewall normals face outward (perpendicular to each edge).
///
/// Vertex format: pos3 + norm3 + uv2 (8 floats, matches `polygon_to_fill_earcut`).
pub fn polygon_to_extrude_earcut(
    ring_lng_lat: &[[f64; 2]],
    zoom: f64,
    center_px: WorldPx,
    base: f32,
    height: f32,
) -> GeoMesh {
    if ring_lng_lat.len() < 3 || height <= 0.0 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let mut points: Vec<[f32; 2]> = ring_lng_lat
        .iter()
        .map(|c| {
            let wp = lng_lat_to_world_px(LngLat::new(c[0], c[1]), zoom);
            [(wp.x - center_px.x) as f32, (wp.y - center_px.y) as f32]
        })
        .collect();
    if points.len() >= 2 && points[0] == points[points.len() - 1] {
        points.pop();
    }
    let n = points.len();
    if n < 3 {
        return GeoMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }

    let top_y = base + height;
    let mut vertices: Vec<f32> = Vec::with_capacity((n + 4 * n) * 8);
    let mut indices: Vec<u32> = Vec::new();

    // ── Roof: CCW-triangulated polygon at y = top_y with normal +Y ──
    for p in points.iter() {
        let u = p[0] / 256.0;
        let v = p[1] / 256.0;
        vertices.extend_from_slice(&[p[0], top_y, p[1], 0.0, 1.0, 0.0, u, v]);
    }
    let signed_area: f32 = (0..n)
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % n];
            (b[0] - a[0]) * (b[1] + a[1])
        })
        .sum::<f32>()
        * 0.5;
    let mut ring: Vec<usize> = if signed_area < 0.0 {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };
    let mut guard = ring.len() * ring.len();
    while ring.len() >= 3 && guard > 0 {
        guard -= 1;
        let mut clipped = false;
        for i in 0..ring.len() {
            let ia = ring[(i + ring.len() - 1) % ring.len()];
            let ib = ring[i];
            let ic = ring[(i + 1) % ring.len()];
            let a = points[ia];
            let b = points[ib];
            let c = points[ic];
            // f64 to keep the sign stable when world-px coords exceed ~1e6.
            let cross = convex_cross_f64(a, b, c);
            if cross <= 0.0 {
                continue;
            }
            let mut contains = false;
            for &j in ring.iter() {
                if j == ia || j == ib || j == ic {
                    continue;
                }
                if point_in_triangle(points[j], a, b, c) {
                    contains = true;
                    break;
                }
            }
            if contains {
                continue;
            }
            indices.push(ia as u32);
            indices.push(ib as u32);
            indices.push(ic as u32);
            ring.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            let first = ring[0] as u32;
            for w in ring.windows(2).skip(1) {
                indices.push(first);
                indices.push(w[0] as u32);
                indices.push(w[1] as u32);
            }
            break;
        }
    }

    // Roof earcut produces CCW-in-2D → N.y < 0 in XZ plane → back-face culled.
    // Reverse winding on roof triangles only; sidewalls are handled separately.
    for tri in indices.chunks_exact_mut(3) {
        tri.swap(1, 2);
    }

    // ── Sidewalls: 2 triangles per edge, outward-facing normal ──
    // Winding is chosen so outward-facing quads are CCW when viewed from outside.
    // Outward = edge-perpendicular pointing away from polygon centroid.
    let cx: f32 = points.iter().map(|p| p[0]).sum::<f32>() / n as f32;
    let cz: f32 = points.iter().map(|p| p[1]).sum::<f32>() / n as f32;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        let ex = b[0] - a[0];
        let ez = b[1] - a[1];
        // Two perpendicular candidates in XZ plane: (-ez, ex) and (ez, -ex).
        // Pick the one pointing away from centroid.
        let mid_x = (a[0] + b[0]) * 0.5;
        let mid_z = (a[1] + b[1]) * 0.5;
        let to_mid_x = mid_x - cx;
        let to_mid_z = mid_z - cz;
        let (nx, nz) = if (-ez) * to_mid_x + ex * to_mid_z > 0.0 {
            (-ez, ex)
        } else {
            (ez, -ex)
        };
        let len = (nx * nx + nz * nz).sqrt().max(1e-6);
        let nx = nx / len;
        let nz = nz / len;

        let base_idx = (vertices.len() / 8) as u32;
        // 4 wall corners: (a,base)=0, (b,base)=1, (b,top)=2, (a,top)=3
        vertices.extend_from_slice(&[a[0], base, a[1], nx, 0.0, nz, 0.0, 0.0]);
        vertices.extend_from_slice(&[b[0], base, b[1], nx, 0.0, nz, 1.0, 0.0]);
        vertices.extend_from_slice(&[b[0], top_y, b[1], nx, 0.0, nz, 1.0, 1.0]);
        vertices.extend_from_slice(&[a[0], top_y, a[1], nx, 0.0, nz, 0.0, 1.0]);
        // Two CCW triangles (viewed from +normal side).
        indices.push(base_idx);
        indices.push(base_idx + 1);
        indices.push(base_idx + 2);
        indices.push(base_idx);
        indices.push(base_idx + 2);
        indices.push(base_idx + 3);
    }

    GeoMesh { vertices, indices }
}
