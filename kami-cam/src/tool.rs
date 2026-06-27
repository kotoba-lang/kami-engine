/// Cutting tool definitions and tool library management.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Classification of cutting tool geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolType {
    EndMill,
    BallNose,
    BullNose,
    Drill,
    Tap,
    FaceMill,
    ChamferMill,
    Lathe,
}

/// Substrate material of the cutting tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolMaterial {
    /// High-Speed Steel
    HSS,
    Carbide,
    Ceramic,
    /// Cubic Boron Nitride
    CBN,
    /// Polycrystalline Diamond
    PCD,
}

/// A single cutting tool with geometry and material properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub id: u32,
    pub name: String,
    pub tool_type: ToolType,
    /// Cutting diameter in mm.
    pub diameter: f64,
    /// Length of the fluted (cutting) portion in mm.
    pub flute_length: f64,
    /// Total tool length in mm.
    pub overall_length: f64,
    /// Number of flutes / cutting edges.
    pub flute_count: u32,
    /// Corner radius in mm (0.0 for sharp corners).
    pub corner_radius: f64,
    pub material: ToolMaterial,
    /// Optional coating description (e.g. "TiAlN", "DLC").
    pub coating: Option<String>,
}

/// In-memory tool library for managing cutting tool inventory.
#[derive(Debug, Clone, Default)]
pub struct ToolLibrary {
    tools: HashMap<u32, Tool>,
}

impl ToolLibrary {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Insert or replace a tool. Returns the previous tool if the id was
    /// already present.
    pub fn add(&mut self, tool: Tool) -> Option<Tool> {
        self.tools.insert(tool.id, tool)
    }

    /// Look up a tool by numeric id.
    pub fn get(&self, id: u32) -> Option<&Tool> {
        self.tools.get(&id)
    }

    /// Remove a tool by id, returning it if found.
    pub fn remove(&mut self, id: u32) -> Option<Tool> {
        self.tools.remove(&id)
    }

    /// Return all tools sorted by id.
    pub fn list(&self) -> Vec<&Tool> {
        let mut v: Vec<&Tool> = self.tools.values().collect();
        v.sort_by_key(|t| t.id);
        v
    }

    /// Number of tools in the library.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the library is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}
