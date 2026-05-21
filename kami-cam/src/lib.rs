/// KAMI CAM — Computer-Aided Manufacturing
///
/// Toolpath generation, G-code output, tool library, stock definition, and CNC
/// post-processor. Accepts geometry as generic input (DVec3 points / mesh data)
/// to avoid circular dependency on kami-cad.

pub mod tool;
pub mod stock;
pub mod toolpath;
pub mod gcode;

pub use tool::{Tool, ToolType, ToolMaterial, ToolLibrary};
pub use stock::{Stock, StockShape, CamMaterial};
pub use toolpath::{CamOperation, CamJob, ToolpathSegment, SegmentType};
pub use gcode::{GcodeConfig, MachineType, PostProcessor, generate_gcode};

// ---------------------------------------------------------------------------
// tool — cutting tool definitions and library
// ---------------------------------------------------------------------------
pub mod tool_impl {
    pub use crate::tool::*;
}

// ---------------------------------------------------------------------------
// stock — workpiece stock definitions and material presets
// ---------------------------------------------------------------------------
pub mod stock_impl {
    pub use crate::stock::*;
}

#[cfg(test)]
mod tests;
