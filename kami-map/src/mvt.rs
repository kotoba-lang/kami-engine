//! Minimal MVT (Mapbox Vector Tile) PBF decoder.
//!
//! Self-contained, no external protobuf dependency. Decodes the subset of the
//! MVT spec used for line/point/polygon geometry extraction:
//! - Tile (3 = repeated Layer)
//! - Layer (1 name, 2 features, 5 extent uint32, 15 version)
//! - Feature (3 type enum, 4 packed uint32 geometry)
//! - GeomType: 1=POINT, 2=LINESTRING, 3=POLYGON
//! - Geometry commands: 1 MoveTo, 2 LineTo, 7 ClosePath with zigzag-encoded deltas
//!
//! Tags / property values are not decoded here — caller filters by layer name only.

use kami_geo::projection::TileCoord;
use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};

#[derive(Debug, Default)]
pub struct VectorFeatures {
    pub lines: Vec<Vec<[f64; 2]>>,
    pub polygons: Vec<Vec<[f64; 2]>>,
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VectorFeatureCollection {
    pub features: Vec<VectorFeature>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VectorFeature {
    pub geometry: VectorGeometry,
    #[serde(default)]
    pub properties: JsonMap<String, JsonValue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "coordinates")]
pub enum VectorGeometry {
    Point([f64; 2]),
    MultiPoint(Vec<[f64; 2]>),
    LineString(Vec<[f64; 2]>),
    MultiLineString(Vec<Vec<[f64; 2]>>),
    Polygon(Vec<Vec<[f64; 2]>>),
}

#[derive(Debug, Clone)]
struct LayerContext {
    keys: Vec<String>,
    values: Vec<JsonValue>,
    extent: u32,
}

/// Decode an MVT PBF blob and convert the named layer's geometries to
/// geographic coordinates within the given tile.
pub fn decode_layer(pbf: &[u8], tile: TileCoord, layer_name: &str) -> VectorFeatures {
    let collection = decode_layer_features(pbf, tile, layer_name);
    let mut out = VectorFeatures::default();
    for feature in collection.features {
        match feature.geometry {
            VectorGeometry::Point(pt) => out.points.push(pt),
            VectorGeometry::MultiPoint(points) => out.points.extend(points),
            VectorGeometry::LineString(line) => out.lines.push(line),
            VectorGeometry::MultiLineString(lines) => out.lines.extend(lines),
            VectorGeometry::Polygon(rings) => out.polygons.extend(rings),
        }
    }
    out
}

/// Decode an MVT PBF blob and return the named layer's features as GeoJSON-like
/// geometry objects plus their decoded properties.
pub fn decode_layer_features(
    pbf: &[u8],
    tile: TileCoord,
    layer_name: &str,
) -> VectorFeatureCollection {
    let mut out = VectorFeatureCollection {
        features: Vec::new(),
    };
    let mut r = PbfReader::new(pbf);
    while let Some((field, wire)) = r.read_tag() {
        if field == 3 && wire == WIRE_LEN {
            let payload = r.read_len_delim();
            let mut lr = PbfReader::new(payload);
            let mut name: Option<&str> = None;
            let mut ctx = LayerContext {
                keys: Vec::new(),
                values: Vec::new(),
                extent: 4096,
            };
            let mut feature_payloads: Vec<&[u8]> = Vec::new();
            while let Some((lf, lw)) = lr.read_tag() {
                match (lf, lw) {
                    (1, WIRE_LEN) => {
                        let raw = lr.read_len_delim();
                        name = std::str::from_utf8(raw).ok();
                    }
                    (2, WIRE_LEN) => feature_payloads.push(lr.read_len_delim()),
                    (3, WIRE_LEN) => {
                        if let Ok(key) = std::str::from_utf8(lr.read_len_delim()) {
                            ctx.keys.push(key.to_string());
                        }
                    }
                    (4, WIRE_LEN) => ctx.values.push(decode_value_message(lr.read_len_delim())),
                    (5, WIRE_VARINT) => ctx.extent = lr.read_varint() as u32,
                    _ => lr.skip_field(lw),
                }
            }
            if name == Some(layer_name) && ctx.extent > 0 {
                for fpl in feature_payloads {
                    if let Some(feature) = decode_feature(fpl, tile, &ctx) {
                        out.features.push(feature);
                    }
                }
            }
        } else {
            r.skip_field(wire);
        }
    }
    out
}

fn decode_feature(payload: &[u8], tile: TileCoord, ctx: &LayerContext) -> Option<VectorFeature> {
    let mut r = PbfReader::new(payload);
    let mut geom_type: u32 = 0;
    let mut tags_payload: Option<&[u8]> = None;
    let mut geometry_payload: Option<&[u8]> = None;
    while let Some((field, wire)) = r.read_tag() {
        match (field, wire) {
            (2, WIRE_LEN) => tags_payload = Some(r.read_len_delim()),
            (3, WIRE_VARINT) => geom_type = r.read_varint() as u32,
            (4, WIRE_LEN) => geometry_payload = Some(r.read_len_delim()),
            _ => r.skip_field(wire),
        }
    }
    let geom = geometry_payload?;
    let properties = decode_feature_tags(tags_payload, ctx);

    // Decode command stream.
    let mut x_local: i32 = 0;
    let mut y_local: i32 = 0;
    let mut current_path: Vec<[f64; 2]> = Vec::new();
    let mut start_pt: Option<[f64; 2]> = None;
    let mut paths: Vec<Vec<[f64; 2]>> = Vec::new();

    let cmds = read_packed_uint32(geom);
    let mut i = 0;

    while i < cmds.len() {
        let header = cmds[i];
        i += 1;
        let cmd = header & 0x7;
        let count = (header >> 3) as usize;
        if cmd == 1 || cmd == 2 {
            for _ in 0..count {
                if i + 1 >= cmds.len() {
                    break;
                }
                let dx = zigzag(cmds[i]);
                let dy = zigzag(cmds[i + 1]);
                i += 2;
                x_local = x_local.wrapping_add(dx);
                y_local = y_local.wrapping_add(dy);
                let geo = tile_local_to_lng_lat(x_local, y_local, ctx.extent, tile);
                if cmd == 1 {
                    // MoveTo: end any open line, start new one (or store point).
                    if !current_path.is_empty() && geom_type == 2 {
                        paths.push(std::mem::take(&mut current_path));
                    }
                    if geom_type == 1 {
                        paths.push(vec![geo]);
                        current_path.clear();
                    } else {
                        current_path.clear();
                        current_path.push(geo);
                        start_pt = Some(geo);
                    }
                } else {
                    current_path.push(geo);
                }
            }
        } else if cmd == 7 {
            // ClosePath (polygon ring close).
            if let Some(s) = start_pt {
                current_path.push(s);
            }
            if !current_path.is_empty() && geom_type == 3 {
                paths.push(std::mem::take(&mut current_path));
            }
            start_pt = None;
        }
    }
    if !current_path.is_empty() {
        paths.push(current_path);
    }

    let geometry = match geom_type {
        1 => {
            if paths.len() == 1 && paths[0].len() == 1 {
                VectorGeometry::Point(paths.pop().unwrap()[0])
            } else {
                VectorGeometry::MultiPoint(
                    paths.into_iter()
                        .filter_map(|mut path| {
                            if path.len() == 1 { path.pop() } else { None }
                        })
                        .collect(),
                )
            }
        }
        2 => {
            if paths.len() == 1 {
                VectorGeometry::LineString(paths.pop().unwrap())
            } else {
                VectorGeometry::MultiLineString(paths)
            }
        }
        3 => VectorGeometry::Polygon(paths),
        _ => return None,
    };

    Some(VectorFeature {
        geometry,
        properties,
    })
}

fn decode_feature_tags(tags_payload: Option<&[u8]>, ctx: &LayerContext) -> JsonMap<String, JsonValue> {
    let mut out = JsonMap::new();
    let Some(tags_buf) = tags_payload else {
        return out;
    };
    let tags = read_packed_uint32(tags_buf);
    for pair in tags.chunks_exact(2) {
        let key = ctx.keys.get(pair[0] as usize);
        let value = ctx.values.get(pair[1] as usize);
        if let (Some(key), Some(value)) = (key, value) {
            out.insert(key.clone(), value.clone());
        }
    }
    out
}

fn decode_value_message(buf: &[u8]) -> JsonValue {
    let mut r = PbfReader::new(buf);
    while let Some((field, wire)) = r.read_tag() {
        match (field, wire) {
            (1, WIRE_LEN) => {
                if let Ok(value) = std::str::from_utf8(r.read_len_delim()) {
                    return JsonValue::String(value.to_string());
                }
                return JsonValue::Null;
            }
            (2, WIRE_32BIT) => {
                let bits = r.read_fixed32();
                return serde_json::Number::from_f64(f32::from_bits(bits) as f64)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null);
            }
            (3, WIRE_64BIT) => {
                let bits = r.read_fixed64();
                return serde_json::Number::from_f64(f64::from_bits(bits))
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null);
            }
            (4, WIRE_VARINT) => return JsonValue::from(r.read_varint() as i64),
            (5, WIRE_VARINT) => return JsonValue::from(r.read_varint()),
            (6, WIRE_VARINT) => return JsonValue::from(zigzag64(r.read_varint())),
            (7, WIRE_VARINT) => return JsonValue::Bool(r.read_varint() != 0),
            _ => r.skip_field(wire),
        }
    }
    JsonValue::Null
}

fn read_packed_uint32(buf: &[u8]) -> Vec<u32> {
    let mut r = PbfReader::new(buf);
    let mut out = Vec::with_capacity(buf.len());
    while !r.eof() {
        out.push(r.read_varint() as u32);
    }
    out
}

fn zigzag(n: u32) -> i32 {
    ((n >> 1) as i32) ^ -((n & 1) as i32)
}

fn zigzag64(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

fn tile_local_to_lng_lat(x: i32, y: i32, extent: u32, tile: TileCoord) -> [f64; 2] {
    // Tile-local [0, extent] → world [0, 2^z] tile space → lng/lat.
    let z = tile.z;
    let n = (1u64 << z) as f64;
    let tx = tile.x as f64 + (x as f64 / extent as f64);
    let ty = tile.y as f64 + (y as f64 / extent as f64);
    let lng = tx / n * 360.0 - 180.0;
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * ty / n)).sinh().atan();
    let lat = lat_rad.to_degrees();
    [lng, lat]
}

// ── Tiny PBF reader ─────────────────────────────────────────────────────

const WIRE_VARINT: u32 = 0;
const WIRE_LEN: u32 = 2;
const WIRE_64BIT: u32 = 1;
const WIRE_32BIT: u32 = 5;

struct PbfReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> PbfReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn eof(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn read_tag(&mut self) -> Option<(u32, u32)> {
        if self.eof() {
            return None;
        }
        let v = self.read_varint();
        Some(((v >> 3) as u32, (v & 0x7) as u32))
    }

    fn read_varint(&mut self) -> u64 {
        let mut shift = 0u32;
        let mut out = 0u64;
        while self.pos < self.buf.len() {
            let b = self.buf[self.pos];
            self.pos += 1;
            out |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift >= 64 {
                break;
            }
        }
        out
    }

    fn read_len_delim(&mut self) -> &'a [u8] {
        let len = self.read_varint() as usize;
        let end = (self.pos + len).min(self.buf.len());
        let s = &self.buf[self.pos..end];
        self.pos = end;
        s
    }

    fn read_fixed32(&mut self) -> u32 {
        let end = (self.pos + 4).min(self.buf.len());
        let mut bytes = [0u8; 4];
        let slice = &self.buf[self.pos..end];
        bytes[..slice.len()].copy_from_slice(slice);
        self.pos = end;
        u32::from_le_bytes(bytes)
    }

    fn read_fixed64(&mut self) -> u64 {
        let end = (self.pos + 8).min(self.buf.len());
        let mut bytes = [0u8; 8];
        let slice = &self.buf[self.pos..end];
        bytes[..slice.len()].copy_from_slice(slice);
        self.pos = end;
        u64::from_le_bytes(bytes)
    }

    fn skip_field(&mut self, wire: u32) {
        match wire {
            WIRE_VARINT => {
                let _ = self.read_varint();
            }
            WIRE_LEN => {
                let _ = self.read_len_delim();
            }
            WIRE_64BIT => self.pos = (self.pos + 8).min(self.buf.len()),
            WIRE_32BIT => self.pos = (self.pos + 4).min(self.buf.len()),
            _ => {} // unknown — stop scanning safely
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_decode() {
        assert_eq!(zigzag(0), 0);
        assert_eq!(zigzag(1), -1);
        assert_eq!(zigzag(2), 1);
        assert_eq!(zigzag(3), -2);
    }

    #[test]
    fn tile_local_origin_is_tile_origin() {
        let t = TileCoord { z: 0, x: 0, y: 0 };
        let [lng, lat] = tile_local_to_lng_lat(0, 0, 4096, t);
        // z=0 origin is (-180, +85.0511).
        assert!((lng - -180.0).abs() < 1e-9);
        assert!((lat - 85.0511287).abs() < 1e-3);
    }

    #[test]
    fn empty_pbf_yields_no_features() {
        let r = decode_layer(&[], TileCoord { z: 0, x: 0, y: 0 }, "anything");
        assert!(r.lines.is_empty() && r.polygons.is_empty() && r.points.is_empty());
    }
}
