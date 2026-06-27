/// Toolpath generation: CAM operations, segment types, and job execution.
use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::stock::Stock;
use crate::tool::ToolLibrary;

/// Pocket clearing strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PocketStrategy {
    Zigzag,
    Spiral,
    TrochoidalPeel,
}

/// 3D surface finishing strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceStrategy {
    Raster,
    Spiral,
    Waterline,
    Pencil,
}

/// Contour side relative to the geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContourSide {
    Inside,
    Outside,
    OnLine,
}

/// A single machining operation with all required parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CamOperation {
    FaceMill {
        tool_id: u32,
        depth_of_cut: f64,
        stepover: f64,
        feed_rate: f64,
        spindle_rpm: f64,
    },
    Pocket {
        tool_id: u32,
        depth: f64,
        stepover: f64,
        strategy: PocketStrategy,
        feed_rate: f64,
        spindle_rpm: f64,
        /// Rectangle pocket boundary: min corner XY.
        pocket_min: DVec3,
        /// Rectangle pocket boundary: max corner XY.
        pocket_max: DVec3,
    },
    Contour {
        tool_id: u32,
        depth: f64,
        side: ContourSide,
        feed_rate: f64,
        spindle_rpm: f64,
    },
    Drill {
        tool_id: u32,
        depth: f64,
        peck_depth: f64,
        feed_rate: f64,
        spindle_rpm: f64,
        holes: Vec<DVec3>,
    },
    Surface3D {
        tool_id: u32,
        stepover: f64,
        strategy: SurfaceStrategy,
        feed_rate: f64,
        spindle_rpm: f64,
    },
    Turn {
        tool_id: u32,
        depth_of_cut: f64,
        feed_rate: f64,
        spindle_rpm: f64,
    },
}

/// Motion type of a single toolpath segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentType {
    /// G00 rapid traverse (no cutting).
    Rapid,
    /// G01 linear interpolation.
    Linear,
    /// G02 clockwise arc.
    ArcCW,
    /// G03 counter-clockwise arc.
    ArcCCW,
}

/// One segment of a generated toolpath.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolpathSegment {
    pub segment_type: SegmentType,
    pub start: DVec3,
    pub end: DVec3,
    /// Feed rate in mm/min (ignored for Rapid).
    pub feed_rate: f64,
    /// Arc center (only meaningful for ArcCW / ArcCCW).
    pub center: Option<DVec3>,
    /// Tool id that produced this segment.
    pub tool_id: u32,
}

/// A complete CAM job combining stock, tool library, and ordered operations.
#[derive(Debug, Clone)]
pub struct CamJob {
    pub stock: Stock,
    pub operations: Vec<CamOperation>,
    pub tool_library: ToolLibrary,
    /// Safe retract height in mm above stock top (Z).
    pub safe_height: f64,
}

impl CamJob {
    pub fn new(stock: Stock, tool_library: ToolLibrary) -> Self {
        Self {
            stock,
            operations: Vec::new(),
            tool_library,
            safe_height: 5.0,
        }
    }

    pub fn add_operation(&mut self, op: CamOperation) {
        self.operations.push(op);
    }

    /// Generate toolpath segments for all operations in order.
    ///
    /// Currently implements zigzag pocket; other operations produce placeholder
    /// rapid moves to the operation location so the G-code structure is valid.
    pub fn generate_toolpath(&self) -> Vec<ToolpathSegment> {
        let mut segments = Vec::new();

        for op in &self.operations {
            match op {
                CamOperation::Pocket {
                    tool_id,
                    depth,
                    stepover,
                    strategy: _,
                    feed_rate,
                    spindle_rpm: _,
                    pocket_min,
                    pocket_max,
                } => {
                    // Zigzag pocket implementation: cut in layers, alternating
                    // X-direction passes with Y stepover.
                    let tool = self.tool_library.get(*tool_id);
                    let tool_radius = tool.map_or(0.0, |t| t.diameter / 2.0);
                    let effective_stepover = if *stepover > 0.0 {
                        *stepover
                    } else {
                        tool_radius
                    };

                    let x_min = pocket_min.x + tool_radius;
                    let x_max = pocket_max.x - tool_radius;
                    let y_min = pocket_min.y + tool_radius;
                    let y_max = pocket_max.y - tool_radius;
                    let z_top = pocket_min.z;
                    let z_bottom = pocket_min.z - depth;
                    let safe_z = z_top + self.safe_height;

                    // Depth layers (simple constant depth-of-cut = stepover for
                    // now; a real implementation would use axial DOC).
                    let layer_doc = effective_stepover.min(*depth);
                    let num_layers = (*depth / layer_doc).ceil() as usize;

                    for layer in 0..num_layers {
                        let z = (z_top - (layer as f64 + 1.0) * layer_doc).max(z_bottom);

                        // Rapid to start of layer
                        let first_start = DVec3::new(x_min, y_min, safe_z);
                        if segments.last().is_some() {
                            let prev_end: DVec3 = last_end(&segments);
                            segments.push(ToolpathSegment {
                                segment_type: SegmentType::Rapid,
                                start: prev_end,
                                end: DVec3::new(x_min, y_min, safe_z),
                                feed_rate: 0.0,
                                center: None,
                                tool_id: *tool_id,
                            });
                        }
                        // Plunge to depth
                        segments.push(ToolpathSegment {
                            segment_type: SegmentType::Rapid,
                            start: first_start,
                            end: DVec3::new(x_min, y_min, z),
                            feed_rate: 0.0,
                            center: None,
                            tool_id: *tool_id,
                        });

                        // Zigzag passes
                        let mut y = y_min;
                        let mut forward = true;
                        while y <= y_max {
                            let (sx, ex) = if forward {
                                (x_min, x_max)
                            } else {
                                (x_max, x_min)
                            };
                            let start = DVec3::new(sx, y, z);
                            let end_pt = DVec3::new(ex, y, z);

                            // Move to pass start
                            let prev = last_end(&segments);
                            if (prev - start).length() > 1e-6 {
                                segments.push(ToolpathSegment {
                                    segment_type: SegmentType::Rapid,
                                    start: prev,
                                    end: start,
                                    feed_rate: 0.0,
                                    center: None,
                                    tool_id: *tool_id,
                                });
                            }

                            // Cutting pass
                            segments.push(ToolpathSegment {
                                segment_type: SegmentType::Linear,
                                start,
                                end: end_pt,
                                feed_rate: *feed_rate,
                                center: None,
                                tool_id: *tool_id,
                            });

                            forward = !forward;
                            y += effective_stepover;
                        }

                        // Retract after layer
                        let prev = last_end(&segments);
                        segments.push(ToolpathSegment {
                            segment_type: SegmentType::Rapid,
                            start: prev,
                            end: DVec3::new(prev.x, prev.y, safe_z),
                            feed_rate: 0.0,
                            center: None,
                            tool_id: *tool_id,
                        });
                    }
                }

                CamOperation::Drill {
                    tool_id,
                    depth,
                    peck_depth,
                    feed_rate,
                    spindle_rpm: _,
                    holes,
                } => {
                    let safe_z = self.safe_height;
                    for hole in holes {
                        let top = DVec3::new(hole.x, hole.y, safe_z);
                        let prev = last_end(&segments);
                        // Rapid to hole position
                        segments.push(ToolpathSegment {
                            segment_type: SegmentType::Rapid,
                            start: prev,
                            end: top,
                            feed_rate: 0.0,
                            center: None,
                            tool_id: *tool_id,
                        });
                        // Peck drill cycles
                        let mut z = hole.z;
                        let z_bottom = hole.z - depth;
                        while z > z_bottom {
                            let target_z = (z - peck_depth).max(z_bottom);
                            segments.push(ToolpathSegment {
                                segment_type: SegmentType::Linear,
                                start: DVec3::new(hole.x, hole.y, z),
                                end: DVec3::new(hole.x, hole.y, target_z),
                                feed_rate: *feed_rate,
                                center: None,
                                tool_id: *tool_id,
                            });
                            // Retract for chip clearing
                            segments.push(ToolpathSegment {
                                segment_type: SegmentType::Rapid,
                                start: DVec3::new(hole.x, hole.y, target_z),
                                end: top,
                                feed_rate: 0.0,
                                center: None,
                                tool_id: *tool_id,
                            });
                            z = target_z;
                        }
                    }
                }

                CamOperation::FaceMill {
                    tool_id,
                    feed_rate: _,
                    ..
                }
                | CamOperation::Contour {
                    tool_id,
                    feed_rate: _,
                    ..
                }
                | CamOperation::Surface3D {
                    tool_id,
                    feed_rate: _,
                    ..
                }
                | CamOperation::Turn {
                    tool_id,
                    feed_rate: _,
                    ..
                } => {
                    // Placeholder: rapid to origin, indicating tool change point.
                    let prev = last_end(&segments);
                    segments.push(ToolpathSegment {
                        segment_type: SegmentType::Rapid,
                        start: prev,
                        end: DVec3::new(0.0, 0.0, self.safe_height),
                        feed_rate: 0.0,
                        center: None,
                        tool_id: *tool_id,
                    });
                }
            }
        }

        segments
    }
}

/// Helper: get the end position of the last segment, or origin if empty.
fn last_end(segments: &[ToolpathSegment]) -> DVec3 {
    segments.last().map(|s| s.end).unwrap_or(DVec3::ZERO)
}
