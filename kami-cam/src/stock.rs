/// Workpiece stock definitions and material presets.

use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Physical shape of the raw stock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StockShape {
    /// Rectangular block defined by width (X), height (Y), depth (Z) in mm.
    Block { width: f64, height: f64, depth: f64 },
    /// Cylindrical billet defined by diameter and length in mm.
    Cylinder { diameter: f64, length: f64 },
    /// Arbitrary mesh input (vertices as DVec3, triangle indices).
    FromMesh {
        vertices: Vec<DVec3>,
        indices: Vec<[u32; 3]>,
    },
}

/// Material properties relevant to CAM feed/speed calculations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CamMaterial {
    pub name: String,
    /// Density in g/cm^3.
    pub density: f64,
    /// Brinell hardness (HB).
    pub hardness: f64,
}

impl CamMaterial {
    pub fn new(name: impl Into<String>, density: f64, hardness: f64) -> Self {
        Self {
            name: name.into(),
            density,
            hardness,
        }
    }

    /// Aluminum 6061-T6.
    pub fn aluminum_6061() -> Self {
        Self::new("Aluminum 6061-T6", 2.70, 95.0)
    }

    /// AISI 1045 medium-carbon steel.
    pub fn steel_1045() -> Self {
        Self::new("Steel 1045", 7.87, 163.0)
    }

    /// Ti-6Al-4V aerospace titanium alloy.
    pub fn titanium_ti6al4v() -> Self {
        Self::new("Titanium Ti-6Al-4V", 4.43, 334.0)
    }

    /// ABS thermoplastic (injection molding / 3D printing stock).
    pub fn abs_plastic() -> Self {
        Self::new("ABS Plastic", 1.04, 10.0)
    }

    /// Red oak hardwood.
    pub fn wood_oak() -> Self {
        Self::new("Oak (Red)", 0.66, 6.0)
    }
}

/// A workpiece (raw stock) to be machined.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stock {
    pub shape: StockShape,
    pub material: CamMaterial,
    /// Stock origin in world coordinates.
    pub origin: DVec3,
}

impl Stock {
    pub fn new(shape: StockShape, material: CamMaterial) -> Self {
        Self {
            shape,
            material,
            origin: DVec3::ZERO,
        }
    }

    pub fn with_origin(mut self, origin: DVec3) -> Self {
        self.origin = origin;
        self
    }

    /// Axis-aligned bounding box dimensions (width, height, depth) in mm.
    /// For `FromMesh`, computes from vertex extents.
    pub fn dimensions(&self) -> DVec3 {
        match &self.shape {
            StockShape::Block { width, height, depth } => DVec3::new(*width, *height, *depth),
            StockShape::Cylinder { diameter, length } => DVec3::new(*diameter, *diameter, *length),
            StockShape::FromMesh { vertices, .. } => {
                if vertices.is_empty() {
                    return DVec3::ZERO;
                }
                let mut min = vertices[0];
                let mut max = vertices[0];
                for v in vertices.iter().skip(1) {
                    min = min.min(*v);
                    max = max.max(*v);
                }
                max - min
            }
        }
    }
}
