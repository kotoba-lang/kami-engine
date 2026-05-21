/// G-code generation from toolpath segments with post-processor configuration.

use serde::{Deserialize, Serialize};
use std::fmt::Write;

use crate::toolpath::{SegmentType, ToolpathSegment};

/// Target CNC machine category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MachineType {
    Mill3Axis,
    Mill4Axis,
    Mill5Axis,
    Lathe,
    LaserCutter,
    Printer3D,
}

/// Controller dialect for G-code formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostProcessor {
    Fanuc,
    Haas,
    Siemens,
    Heidenhain,
    LinuxCNC,
    Marlin,
    Grbl,
}

/// Units for G-code output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GcodeUnits {
    Millimeters,
    Inches,
}

/// Coordinate system (G54-G59).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinateSystem {
    G54,
    G55,
    G56,
    G57,
    G58,
    G59,
}

impl std::fmt::Display for CoordinateSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::G54 => write!(f, "G54"),
            Self::G55 => write!(f, "G55"),
            Self::G56 => write!(f, "G56"),
            Self::G57 => write!(f, "G57"),
            Self::G58 => write!(f, "G58"),
            Self::G59 => write!(f, "G59"),
        }
    }
}

/// Configuration for G-code generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcodeConfig {
    pub machine_type: MachineType,
    pub post_processor: PostProcessor,
    pub units: GcodeUnits,
    /// Safe retract height (Z) in work coordinates.
    pub safe_height: f64,
    pub coordinate_system: CoordinateSystem,
    /// Program number (Fanuc/Haas O-number).
    pub program_number: u32,
    /// Enable coolant (M08/M09).
    pub coolant: bool,
}

impl Default for GcodeConfig {
    fn default() -> Self {
        Self {
            machine_type: MachineType::Mill3Axis,
            post_processor: PostProcessor::Fanuc,
            units: GcodeUnits::Millimeters,
            safe_height: 5.0,
            coordinate_system: CoordinateSystem::G54,
            program_number: 1,
            coolant: true,
        }
    }
}

/// Generate G-code string from toolpath segments and configuration.
///
/// Produces valid G-code with:
/// - Program header (O-number, units, coordinate system, absolute mode)
/// - Tool changes (T/M06) when tool_id changes between segments
/// - Spindle start (M03) / stop (M05)
/// - Coolant on (M08) / off (M09)
/// - Motion commands: G00 (rapid), G01 (linear), G02 (arc CW), G03 (arc CCW)
/// - Program end (M30)
pub fn generate_gcode(segments: &[ToolpathSegment], config: &GcodeConfig) -> String {
    let mut out = String::with_capacity(segments.len() * 64);

    // --- Header ---
    let _ = writeln!(out, "%");
    let _ = writeln!(out, "O{:04}", config.program_number);
    let _ = writeln!(out, "(KAMI CAM — generated G-code)");

    // Units
    match config.units {
        GcodeUnits::Millimeters => { let _ = writeln!(out, "G21 (metric)"); }
        GcodeUnits::Inches => { let _ = writeln!(out, "G20 (imperial)"); }
    }

    // Absolute positioning
    let _ = writeln!(out, "G90 (absolute)");
    // Coordinate system
    let _ = writeln!(out, "{}", config.coordinate_system);
    // Cancel cutter compensation
    let _ = writeln!(out, "G40 (cancel cutter comp)");
    // Cancel tool length offset
    let _ = writeln!(out, "G49 (cancel tool length offset)");
    // Initial safe retract
    let _ = writeln!(out, "G00 Z{:.4}", config.safe_height);

    // --- Body ---
    let mut current_tool: Option<u32> = None;
    let mut spindle_on = false;

    for seg in segments {
        // Tool change when tool_id differs
        if current_tool != Some(seg.tool_id) {
            // Stop spindle and coolant before tool change
            if spindle_on {
                let _ = writeln!(out, "M05 (spindle stop)");
                if config.coolant {
                    let _ = writeln!(out, "M09 (coolant off)");
                }
                spindle_on = false;
            }
            // Retract before tool change
            let _ = writeln!(out, "G00 Z{:.4}", config.safe_height);
            let _ = writeln!(out, "T{:02} M06 (tool change)", seg.tool_id);
            // Start spindle (default CW) — feed_rate on first linear will set S
            // Use a nominal RPM; real post-processor would pull from operation.
            let _ = writeln!(out, "M03 S10000 (spindle CW)");
            if config.coolant {
                let _ = writeln!(out, "M08 (coolant on)");
            }
            spindle_on = true;
            current_tool = Some(seg.tool_id);
        }

        match seg.segment_type {
            SegmentType::Rapid => {
                let _ = writeln!(
                    out,
                    "G00 X{:.4} Y{:.4} Z{:.4}",
                    seg.end.x, seg.end.y, seg.end.z
                );
            }
            SegmentType::Linear => {
                let _ = writeln!(
                    out,
                    "G01 X{:.4} Y{:.4} Z{:.4} F{:.1}",
                    seg.end.x, seg.end.y, seg.end.z, seg.feed_rate
                );
            }
            SegmentType::ArcCW => {
                if let Some(c) = seg.center {
                    let i = c.x - seg.start.x;
                    let j = c.y - seg.start.y;
                    let _ = writeln!(
                        out,
                        "G02 X{:.4} Y{:.4} Z{:.4} I{:.4} J{:.4} F{:.1}",
                        seg.end.x, seg.end.y, seg.end.z, i, j, seg.feed_rate
                    );
                }
            }
            SegmentType::ArcCCW => {
                if let Some(c) = seg.center {
                    let i = c.x - seg.start.x;
                    let j = c.y - seg.start.y;
                    let _ = writeln!(
                        out,
                        "G03 X{:.4} Y{:.4} Z{:.4} I{:.4} J{:.4} F{:.1}",
                        seg.end.x, seg.end.y, seg.end.z, i, j, seg.feed_rate
                    );
                }
            }
        }
    }

    // --- Footer ---
    if spindle_on {
        let _ = writeln!(out, "M05 (spindle stop)");
        if config.coolant {
            let _ = writeln!(out, "M09 (coolant off)");
        }
    }
    let _ = writeln!(out, "G00 Z{:.4} (retract)", config.safe_height);
    let _ = writeln!(out, "G00 X0.0000 Y0.0000 (return to origin)");
    let _ = writeln!(out, "M30 (program end)");
    let _ = writeln!(out, "%");

    out
}
