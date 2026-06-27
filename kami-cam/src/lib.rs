pub mod gcode;
pub mod stock;
/// KAMI CAM — Computer-Aided Manufacturing
///
/// Toolpath generation, G-code output, tool library, stock definition, and CNC
/// post-processor. Accepts geometry as generic input (DVec3 points / mesh data)
/// to avoid circular dependency on kami-cad.
pub mod tool;
pub mod toolpath;

pub use gcode::{GcodeConfig, MachineType, PostProcessor, generate_gcode};
pub use stock::{CamMaterial, Stock, StockShape};
pub use tool::{Tool, ToolLibrary, ToolMaterial, ToolType};
pub use toolpath::{CamJob, CamOperation, SegmentType, ToolpathSegment};

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
