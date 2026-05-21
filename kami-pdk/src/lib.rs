/// KAMI PDK — Process Design Kit management.
///
/// Technology node definitions, Liberty timing characterisation, LEF physical
/// layout abstractions, standard cell libraries with predefined generic cells,
/// and a statistical memory compiler for SRAM/ROM/RegFile estimation.

pub use technology::{TechNode, TechFile, LayerDef, LayerType};
pub use liberty::{LibertyLibrary, LibertyCell, LibertyPin, TimingArc, LookupTable, PinDirection, TimingType};
pub use lef::{LefLibrary, LefMacro, LefPin, LefRect, MacroClass};
pub use stdcell::{StdCellLibrary, StdCell, CellFunction};
pub use memory::{MemoryType, MemorySpec, MemoryCompilerResult, compile_memory};

// ── technology ───────────────────────────────────────────────────────────────

pub mod technology {
    use serde::{Serialize, Deserialize};
    use std::collections::HashMap;

    /// Semiconductor technology node.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum TechNode {
        N180, N130, N90, N65, N45, N28, N22, N16, N14, N10, N7, N5, N3, N2,
    }

    impl TechNode {
        /// Feature size in nanometres.
        pub fn feature_nm(&self) -> u32 {
            match self {
                Self::N180 => 180, Self::N130 => 130, Self::N90 => 90,
                Self::N65 => 65, Self::N45 => 45, Self::N28 => 28,
                Self::N22 => 22, Self::N16 => 16, Self::N14 => 14,
                Self::N10 => 10, Self::N7 => 7, Self::N5 => 5,
                Self::N3 => 3, Self::N2 => 2,
            }
        }
    }

    /// Physical layer type.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum LayerType {
        Diffusion,
        Poly,
        Metal,
        Via,
        Implant,
        Well,
    }

    /// GDS layer definition.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LayerDef {
        pub name: String,
        pub gds_number: u32,
        pub gds_datatype: u32,
        pub layer_type: LayerType,
    }

    /// Technology file describing design rules for a given node.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TechFile {
        pub node: TechNode,
        pub num_metal_layers: u32,
        pub min_width: HashMap<String, f64>,
        pub min_spacing: HashMap<String, f64>,
        pub grid_unit_nm: f64,
        pub layer_map: Vec<LayerDef>,
    }

    impl TechFile {
        /// Get minimum width for a layer (in micrometres).
        pub fn get_min_width(&self, layer: &str) -> Option<f64> {
            self.min_width.get(layer).copied()
        }

        /// Get minimum spacing for a layer (in micrometres).
        pub fn get_min_spacing(&self, layer: &str) -> Option<f64> {
            self.min_spacing.get(layer).copied()
        }

        /// Compute metal pitch for a given metal layer number.
        ///
        /// Pitch = min_width + min_spacing for that metal layer.
        pub fn metal_pitch(&self, layer_num: u32) -> Option<f64> {
            let key = format!("metal{}", layer_num);
            let w = self.min_width.get(&key)?;
            let s = self.min_spacing.get(&key)?;
            Some(w + s)
        }

        /// Create a representative tech file for common nodes.
        pub fn for_node(node: TechNode) -> Self {
            let feat = node.feature_nm() as f64 / 1000.0; // µm
            let num_metals = match node {
                TechNode::N180 | TechNode::N130 => 6,
                TechNode::N90 | TechNode::N65 => 8,
                TechNode::N45 | TechNode::N28 => 9,
                _ => 12,
            };
            let mut min_width = HashMap::new();
            let mut min_spacing = HashMap::new();
            for i in 1..=num_metals {
                let key = format!("metal{}", i);
                let w = feat * (1.0 + 0.1 * i as f64);
                min_width.insert(key.clone(), w);
                min_spacing.insert(key, w * 0.8);
            }
            min_width.insert("poly".into(), feat);
            min_spacing.insert("poly".into(), feat * 1.2);
            min_width.insert("diffusion".into(), feat * 1.5);
            min_spacing.insert("diffusion".into(), feat * 1.5);

            let layer_map = vec![
                LayerDef { name: "nwell".into(), gds_number: 1, gds_datatype: 0, layer_type: LayerType::Well },
                LayerDef { name: "diffusion".into(), gds_number: 2, gds_datatype: 0, layer_type: LayerType::Diffusion },
                LayerDef { name: "poly".into(), gds_number: 3, gds_datatype: 0, layer_type: LayerType::Poly },
                LayerDef { name: "metal1".into(), gds_number: 10, gds_datatype: 0, layer_type: LayerType::Metal },
                LayerDef { name: "via1".into(), gds_number: 11, gds_datatype: 0, layer_type: LayerType::Via },
                LayerDef { name: "metal2".into(), gds_number: 12, gds_datatype: 0, layer_type: LayerType::Metal },
            ];

            Self {
                node,
                num_metal_layers: num_metals,
                min_width,
                min_spacing,
                grid_unit_nm: node.feature_nm() as f64 / 4.0,
                layer_map,
            }
        }
    }
}

// ── liberty ──────────────────────────────────────────────────────────────────

pub mod liberty {
    use serde::{Serialize, Deserialize};

    /// Pin direction.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum PinDirection {
        Input,
        Output,
        Inout,
    }

    /// Timing arc type.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum TimingType {
        Combinational,
        RisingEdge,
        FallingEdge,
        Setup,
        Hold,
    }

    /// 2D lookup table for delay/transition characterisation.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LookupTable {
        pub index1: Vec<f64>,
        pub index2: Vec<f64>,
        pub values: Vec<Vec<f64>>,
    }

    impl LookupTable {
        /// Simple 1×1 table from a scalar value.
        pub fn scalar(val: f64) -> Self {
            Self {
                index1: vec![0.0],
                index2: vec![0.0],
                values: vec![vec![val]],
            }
        }
    }

    /// A pin within a Liberty cell.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LibertyPin {
        pub name: String,
        pub direction: PinDirection,
        pub capacitance: f64,
        pub max_transition: f64,
        pub function: Option<String>,
    }

    /// A timing arc between two pins.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TimingArc {
        pub from_pin: String,
        pub to_pin: String,
        pub timing_type: TimingType,
        pub cell_rise: LookupTable,
        pub cell_fall: LookupTable,
        pub rise_transition: LookupTable,
        pub fall_transition: LookupTable,
    }

    /// A single cell in a Liberty library.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LibertyCell {
        pub name: String,
        pub area: f64,
        pub pins: Vec<LibertyPin>,
        pub timing_arcs: Vec<TimingArc>,
        pub leakage_power: f64,
    }

    /// Liberty timing library.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LibertyLibrary {
        pub name: String,
        pub cells: Vec<LibertyCell>,
        pub nom_voltage: f64,
        pub nom_temperature: f64,
        pub time_unit: String,
        pub capacitance_unit: String,
    }

    impl LibertyLibrary {
        pub fn find_cell(&self, name: &str) -> Option<&LibertyCell> {
            self.cells.iter().find(|c| c.name == name)
        }

        pub fn cell_count(&self) -> usize {
            self.cells.len()
        }

        pub fn total_area(&self) -> f64 {
            self.cells.iter().map(|c| c.area).sum()
        }
    }

    /// Parse a basic Liberty (.lib) format string.
    ///
    /// Recognises `library`, `cell`, `pin`, `timing` blocks with key attributes.
    /// This is a simplified parser sufficient for common Liberty files.
    pub fn parse_liberty(input: &str) -> LibertyLibrary {
        let mut lib = LibertyLibrary {
            name: String::new(),
            cells: Vec::new(),
            nom_voltage: 1.1,
            nom_temperature: 25.0,
            time_unit: "1ns".into(),
            capacitance_unit: "1pF".into(),
        };

        let mut current_cell: Option<LibertyCell> = None;
        let mut current_pin: Option<LibertyPin> = None;
        let mut in_timing = false;
        let mut current_arc: Option<TimingArc> = None;

        for line in input.lines() {
            let trimmed = line.trim();

            // Library name.
            if trimmed.starts_with("library") {
                if let Some(name) = extract_paren_name(trimmed) {
                    lib.name = name;
                }
                continue;
            }

            if trimmed.starts_with("cell") && trimmed.contains('(') {
                // Flush previous cell.
                if let Some(mut cell) = current_cell.take() {
                    if let Some(pin) = current_pin.take() { cell.pins.push(pin); }
                    if let Some(arc) = current_arc.take() { cell.timing_arcs.push(arc); }
                    lib.cells.push(cell);
                }
                let name = extract_paren_name(trimmed).unwrap_or_default();
                current_cell = Some(LibertyCell {
                    name,
                    area: 0.0,
                    pins: Vec::new(),
                    timing_arcs: Vec::new(),
                    leakage_power: 0.0,
                });
                in_timing = false;
                continue;
            }

            if trimmed.starts_with("pin") && trimmed.contains('(') && current_cell.is_some() {
                if let Some(pin) = current_pin.take() {
                    if let Some(ref mut cell) = current_cell { cell.pins.push(pin); }
                }
                if let Some(arc) = current_arc.take() {
                    if let Some(ref mut cell) = current_cell { cell.timing_arcs.push(arc); }
                }
                in_timing = false;
                let name = extract_paren_name(trimmed).unwrap_or_default();
                current_pin = Some(LibertyPin {
                    name,
                    direction: PinDirection::Input,
                    capacitance: 0.0,
                    max_transition: 0.0,
                    function: None,
                });
                continue;
            }

            if trimmed.starts_with("timing") && trimmed.contains('(') {
                in_timing = true;
                current_arc = Some(TimingArc {
                    from_pin: String::new(),
                    to_pin: current_pin.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
                    timing_type: TimingType::Combinational,
                    cell_rise: LookupTable::scalar(0.0),
                    cell_fall: LookupTable::scalar(0.0),
                    rise_transition: LookupTable::scalar(0.0),
                    fall_transition: LookupTable::scalar(0.0),
                });
                continue;
            }

            // Attribute parsing.
            if let Some((key, val)) = parse_attr(trimmed) {
                match key.as_str() {
                    "nom_voltage" => { lib.nom_voltage = val.parse().unwrap_or(lib.nom_voltage); }
                    "nom_temperature" => { lib.nom_temperature = val.parse().unwrap_or(lib.nom_temperature); }
                    "time_unit" => { lib.time_unit = val.trim_matches('"').to_string(); }
                    "capacitance_unit" => { lib.capacitance_unit = val.trim_matches('"').to_string(); }
                    "area" if current_cell.is_some() => {
                        if let Some(ref mut cell) = current_cell {
                            cell.area = val.parse().unwrap_or(0.0);
                        }
                    }
                    "cell_leakage_power" if current_cell.is_some() => {
                        if let Some(ref mut cell) = current_cell {
                            cell.leakage_power = val.parse().unwrap_or(0.0);
                        }
                    }
                    "direction" if current_pin.is_some() => {
                        if let Some(ref mut pin) = current_pin {
                            pin.direction = match val.trim().to_lowercase().as_str() {
                                "output" => PinDirection::Output,
                                "inout" => PinDirection::Inout,
                                _ => PinDirection::Input,
                            };
                        }
                    }
                    "capacitance" if current_pin.is_some() && !in_timing => {
                        if let Some(ref mut pin) = current_pin {
                            pin.capacitance = val.parse().unwrap_or(0.0);
                        }
                    }
                    "max_transition" if current_pin.is_some() => {
                        if let Some(ref mut pin) = current_pin {
                            pin.max_transition = val.parse().unwrap_or(0.0);
                        }
                    }
                    "function" if current_pin.is_some() => {
                        if let Some(ref mut pin) = current_pin {
                            pin.function = Some(val.trim_matches('"').to_string());
                        }
                    }
                    "related_pin" if in_timing => {
                        if let Some(ref mut arc) = current_arc {
                            arc.from_pin = val.trim_matches('"').to_string();
                        }
                    }
                    _ => {}
                }
            }
        }

        // Flush last cell.
        if let Some(mut cell) = current_cell {
            if let Some(pin) = current_pin { cell.pins.push(pin); }
            if let Some(arc) = current_arc { cell.timing_arcs.push(arc); }
            lib.cells.push(cell);
        }
        lib
    }

    fn extract_paren_name(s: &str) -> Option<String> {
        let start = s.find('(')? + 1;
        let end = s.find(')')?;
        Some(s[start..end].trim().trim_matches('"').to_string())
    }

    fn parse_attr(s: &str) -> Option<(String, String)> {
        let colon = s.find(':')?;
        let key = s[..colon].trim().to_string();
        let val = s[colon + 1..].trim().trim_end_matches(';').trim().to_string();
        Some((key, val))
    }
}

// ── lef ──────────────────────────────────────────────────────────────────────

pub mod lef {
    use serde::{Serialize, Deserialize};

    /// LEF macro class.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum MacroClass {
        Core,
        Block,
        Pad,
        Endcap,
    }

    /// A rectangle on a specific layer.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LefRect {
        pub layer: String,
        /// (x1, y1, x2, y2) in micrometres.
        pub rect: (f64, f64, f64, f64),
    }

    /// A pin in a LEF macro.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LefPin {
        pub name: String,
        pub direction: String,
        pub port: Vec<LefRect>,
    }

    /// A LEF macro (cell physical abstract).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LefMacro {
        pub name: String,
        pub class: MacroClass,
        pub size: (f64, f64),
        pub symmetry: String,
        pub site: String,
        pub pins: Vec<LefPin>,
        pub obs: Vec<LefRect>,
    }

    /// Collection of LEF macros.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct LefLibrary {
        pub macros: Vec<LefMacro>,
    }

    impl LefLibrary {
        pub fn find_macro(&self, name: &str) -> Option<&LefMacro> {
            self.macros.iter().find(|m| m.name == name)
        }
    }

    /// Parse a basic LEF format string.
    ///
    /// Recognises MACRO, CLASS, SIZE, SYMMETRY, SITE, PIN, OBS blocks.
    pub fn parse_lef(input: &str) -> LefLibrary {
        let mut lib = LefLibrary::default();
        let mut current_macro: Option<LefMacro> = None;
        let mut current_pin: Option<LefPin> = None;
        let mut in_obs = false;
        let mut current_layer = String::new();

        for line in input.lines() {
            let trimmed = line.trim();
            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            if tokens.is_empty() { continue; }

            match tokens[0] {
                "MACRO" if tokens.len() >= 2 => {
                    if let Some(m) = current_macro.take() { lib.macros.push(m); }
                    current_macro = Some(LefMacro {
                        name: tokens[1].to_string(),
                        class: MacroClass::Core,
                        size: (0.0, 0.0),
                        symmetry: String::new(),
                        site: String::new(),
                        pins: Vec::new(),
                        obs: Vec::new(),
                    });
                    in_obs = false;
                }
                "CLASS" if current_macro.is_some() && tokens.len() >= 2 => {
                    if let Some(ref mut m) = current_macro {
                        m.class = match tokens[1].to_uppercase().as_str() {
                            "BLOCK" => MacroClass::Block,
                            "PAD" => MacroClass::Pad,
                            "ENDCAP" => MacroClass::Endcap,
                            _ => MacroClass::Core,
                        };
                    }
                }
                "SIZE" if current_macro.is_some() && tokens.len() >= 4 => {
                    if let Some(ref mut m) = current_macro {
                        let w: f64 = tokens[1].parse().unwrap_or(0.0);
                        let h: f64 = tokens[3].parse().unwrap_or(0.0);
                        m.size = (w, h);
                    }
                }
                "SYMMETRY" if current_macro.is_some() && tokens.len() >= 2 => {
                    if let Some(ref mut m) = current_macro {
                        m.symmetry = tokens[1..].join(" ").trim_end_matches(';').trim().to_string();
                    }
                }
                "SITE" if current_macro.is_some() && tokens.len() >= 2 => {
                    if let Some(ref mut m) = current_macro {
                        m.site = tokens[1].trim_end_matches(';').to_string();
                    }
                }
                "PIN" if current_macro.is_some() && tokens.len() >= 2 => {
                    if let Some(pin) = current_pin.take() {
                        if let Some(ref mut m) = current_macro { m.pins.push(pin); }
                    }
                    in_obs = false;
                    current_pin = Some(LefPin {
                        name: tokens[1].to_string(),
                        direction: "INPUT".into(),
                        port: Vec::new(),
                    });
                }
                "DIRECTION" if current_pin.is_some() && tokens.len() >= 2 => {
                    if let Some(ref mut pin) = current_pin {
                        pin.direction = tokens[1].trim_end_matches(';').to_string();
                    }
                }
                "OBS" if current_macro.is_some() => {
                    if let Some(pin) = current_pin.take() {
                        if let Some(ref mut m) = current_macro { m.pins.push(pin); }
                    }
                    in_obs = true;
                }
                "LAYER" if tokens.len() >= 2 => {
                    current_layer = tokens[1].trim_end_matches(';').to_string();
                }
                "RECT" if tokens.len() >= 5 => {
                    let x1: f64 = tokens[1].parse().unwrap_or(0.0);
                    let y1: f64 = tokens[2].parse().unwrap_or(0.0);
                    let x2: f64 = tokens[3].parse().unwrap_or(0.0);
                    let y2: f64 = tokens[4].trim_end_matches(';').parse().unwrap_or(0.0);
                    let r = LefRect { layer: current_layer.clone(), rect: (x1, y1, x2, y2) };
                    if in_obs {
                        if let Some(ref mut m) = current_macro { m.obs.push(r); }
                    } else if let Some(ref mut pin) = current_pin {
                        pin.port.push(r);
                    }
                }
                "END" => {
                    // END of PIN, OBS, or MACRO.
                    if in_obs && tokens.len() == 1 {
                        in_obs = false;
                    } else if current_pin.is_some() && tokens.len() >= 2 && tokens[1] != current_macro.as_ref().map(|m| m.name.as_str()).unwrap_or("") {
                        if let Some(pin) = current_pin.take() {
                            if let Some(ref mut m) = current_macro { m.pins.push(pin); }
                        }
                    } else if tokens.len() >= 2 {
                        // END <macro_name>
                        if let Some(pin) = current_pin.take() {
                            if let Some(ref mut m) = current_macro { m.pins.push(pin); }
                        }
                        if let Some(m) = current_macro.take() { lib.macros.push(m); }
                    }
                }
                _ => {}
            }
        }
        if let Some(mut m) = current_macro {
            if let Some(pin) = current_pin { m.pins.push(pin); }
            lib.macros.push(m);
        }
        lib
    }
}

// ── stdcell ──────────────────────────────────────────────────────────────────

pub mod stdcell {
    use serde::{Serialize, Deserialize};
    use crate::technology::TechNode;

    /// Standard cell logic function.
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum CellFunction {
        Inv, Nand2, Nand3, Nor2, Nor3, And2, Or2, Xor2,
        Buf, Dff, Latch, Mux2, Aoi21, Oai21, TieHi, TieLo,
    }

    /// A standard cell definition.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StdCell {
        pub name: String,
        pub function: CellFunction,
        pub drive_strength: u32,
        pub area: f64,
        pub input_pins: Vec<String>,
        pub output_pins: Vec<String>,
    }

    /// Standard cell library.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StdCellLibrary {
        pub name: String,
        pub tech_node: TechNode,
        pub cells: Vec<StdCell>,
    }

    impl StdCellLibrary {
        /// Find cells by logic function.
        pub fn find_by_function(&self, func: &CellFunction) -> Vec<&StdCell> {
            self.cells.iter().filter(|c| &c.function == func).collect()
        }

        /// Return cells sorted by area (ascending).
        pub fn cells_sorted_by_area(&self) -> Vec<&StdCell> {
            let mut sorted: Vec<&StdCell> = self.cells.iter().collect();
            sorted.sort_by(|a, b| a.area.partial_cmp(&b.area).unwrap_or(std::cmp::Ordering::Equal));
            sorted
        }
    }

    /// Create a generic standard cell library with ~20 cells at realistic areas.
    pub fn create_generic_lib(node: TechNode) -> StdCellLibrary {
        let scale = node.feature_nm() as f64 / 7.0; // normalise to N7
        let cell = |name: &str, func: CellFunction, drive: u32, base_area: f64,
                     inputs: &[&str], outputs: &[&str]| -> StdCell {
            StdCell {
                name: name.to_string(),
                function: func,
                drive_strength: drive,
                area: base_area * scale * scale,
                input_pins: inputs.iter().map(|s| s.to_string()).collect(),
                output_pins: outputs.iter().map(|s| s.to_string()).collect(),
            }
        };

        let cells = vec![
            cell("INV_X1",   CellFunction::Inv,   1, 0.798,  &["A"],         &["Y"]),
            cell("INV_X2",   CellFunction::Inv,   2, 1.064,  &["A"],         &["Y"]),
            cell("INV_X4",   CellFunction::Inv,   4, 1.596,  &["A"],         &["Y"]),
            cell("BUF_X1",   CellFunction::Buf,   1, 1.596,  &["A"],         &["Y"]),
            cell("BUF_X2",   CellFunction::Buf,   2, 2.128,  &["A"],         &["Y"]),
            cell("NAND2_X1", CellFunction::Nand2, 1, 1.064,  &["A", "B"],    &["Y"]),
            cell("NAND2_X2", CellFunction::Nand2, 2, 1.596,  &["A", "B"],    &["Y"]),
            cell("NAND3_X1", CellFunction::Nand3, 1, 1.330,  &["A","B","C"], &["Y"]),
            cell("NOR2_X1",  CellFunction::Nor2,  1, 1.064,  &["A", "B"],    &["Y"]),
            cell("NOR2_X2",  CellFunction::Nor2,  2, 1.596,  &["A", "B"],    &["Y"]),
            cell("NOR3_X1",  CellFunction::Nor3,  1, 1.330,  &["A","B","C"], &["Y"]),
            cell("AND2_X1",  CellFunction::And2,  1, 1.596,  &["A", "B"],    &["Y"]),
            cell("OR2_X1",   CellFunction::Or2,   1, 1.596,  &["A", "B"],    &["Y"]),
            cell("XOR2_X1",  CellFunction::Xor2,  1, 2.660,  &["A", "B"],    &["Y"]),
            cell("AOI21_X1", CellFunction::Aoi21, 1, 1.330,  &["A","B","C"], &["Y"]),
            cell("OAI21_X1", CellFunction::Oai21, 1, 1.330,  &["A","B","C"], &["Y"]),
            cell("MUX2_X1",  CellFunction::Mux2,  1, 2.660,  &["A","B","S"], &["Y"]),
            cell("DFF_X1",   CellFunction::Dff,   1, 4.256,  &["D", "CK"],   &["Q"]),
            cell("LATCH_X1", CellFunction::Latch, 1, 3.192,  &["D", "G"],    &["Q"]),
            cell("TIEHI_X1", CellFunction::TieHi, 1, 0.798,  &[],            &["Y"]),
            cell("TIELO_X1", CellFunction::TieLo, 1, 0.798,  &[],            &["Y"]),
        ];

        StdCellLibrary {
            name: format!("GFTD_GENERIC_{:?}", node),
            tech_node: node,
            cells,
        }
    }
}

// ── memory ───────────────────────────────────────────────────────────────────

pub mod memory {
    use crate::technology::TechNode;
    use serde::{Serialize, Deserialize};

    /// Memory macro type.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum MemoryType {
        Sram,
        Rom,
        RegFile,
    }

    /// Memory specification input.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MemorySpec {
        pub mem_type: MemoryType,
        pub words: u32,
        pub bits: u32,
        pub mux: u32,
        pub banks: u32,
    }

    /// Memory compiler output.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MemoryCompilerResult {
        pub name: String,
        pub area_um2: f64,
        pub read_time_ns: f64,
        pub write_time_ns: f64,
        pub leakage_uw: f64,
        pub pins: Vec<String>,
    }

    /// Compile a memory specification into estimated area and timing.
    ///
    /// Uses statistical models derived from published SRAM/ROM bitcell sizes
    /// scaled by technology node.
    pub fn compile_memory(spec: &MemorySpec, tech: TechNode) -> MemoryCompilerResult {
        let feat = tech.feature_nm() as f64;

        // Bitcell area in µm² (6T SRAM reference: ~0.05 µm² at N7).
        let bitcell_area = match spec.mem_type {
            MemoryType::Sram => 0.05 * (feat / 7.0) * (feat / 7.0),
            MemoryType::Rom => 0.02 * (feat / 7.0) * (feat / 7.0),
            MemoryType::RegFile => 0.10 * (feat / 7.0) * (feat / 7.0),
        };

        let total_bits = spec.words as f64 * spec.bits as f64 * spec.banks as f64;
        // Peripheral overhead ~30% for SRAM, ~15% for ROM, ~40% for RegFile.
        let overhead = match spec.mem_type {
            MemoryType::Sram => 1.30,
            MemoryType::Rom => 1.15,
            MemoryType::RegFile => 1.40,
        };
        let area_um2 = total_bits * bitcell_area * overhead;

        // Timing estimate (ns): base + log2(words) scaling.
        let log2_words = (spec.words as f64).log2().max(1.0);
        let timing_scale = feat / 7.0;
        let read_time_ns = (0.2 + 0.05 * log2_words) * timing_scale;
        let write_time_ns = read_time_ns * match spec.mem_type {
            MemoryType::Sram => 1.1,
            MemoryType::Rom => 0.0, // ROM is read-only
            MemoryType::RegFile => 0.9,
        };

        // Leakage (µW): proportional to total bits and node.
        let leakage_per_bit_uw = 1e-6 * (feat / 7.0); // ~1 nW/bit at N7
        let leakage_uw = total_bits * leakage_per_bit_uw;

        // Pin list.
        let mut pins = vec![
            "CLK".into(), "CEN".into(), "WEN".into(),
        ];
        for i in 0..addr_bits(spec.words) {
            pins.push(format!("A[{}]", i));
        }
        for i in 0..spec.bits {
            pins.push(format!("D[{}]", i));
            pins.push(format!("Q[{}]", i));
        }

        let name = format!(
            "{}_{}x{}m{}b{}",
            match spec.mem_type {
                MemoryType::Sram => "SRAM",
                MemoryType::Rom => "ROM",
                MemoryType::RegFile => "RF",
            },
            spec.words, spec.bits, spec.mux, spec.banks
        );

        MemoryCompilerResult {
            name,
            area_um2,
            read_time_ns,
            write_time_ns,
            leakage_uw,
            pins,
        }
    }

    fn addr_bits(words: u32) -> u32 {
        if words <= 1 { return 1; }
        32 - (words - 1).leading_zeros()
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tech_file_min_width() {
        let tf = TechFile::for_node(TechNode::N7);
        assert!(tf.get_min_width("poly").is_some());
        let w = tf.get_min_width("poly").unwrap();
        assert!(w > 0.0 && w < 1.0, "poly min_width should be sub-micron, got {}", w);
        assert!(tf.metal_pitch(1).is_some());
    }

    #[test]
    fn liberty_cell_lookup() {
        let lib_str = r#"
library (test_lib) {
  nom_voltage : 0.9 ;
  nom_temperature : 25 ;
  time_unit : "1ns" ;
  cell (INV_X1) {
    area : 0.8 ;
    cell_leakage_power : 0.001 ;
    pin (A) {
      direction : input ;
      capacitance : 0.002 ;
    }
    pin (Y) {
      direction : output ;
      function : "!A" ;
    }
  }
  cell (NAND2_X1) {
    area : 1.2 ;
  }
}
"#;
        let lib = liberty::parse_liberty(lib_str);
        assert_eq!(lib.cell_count(), 2);
        let inv = lib.find_cell("INV_X1").expect("INV_X1 not found");
        assert!((inv.area - 0.8).abs() < 1e-6);
        assert!(lib.find_cell("NAND2_X1").is_some());
        assert!(lib.find_cell("MISSING").is_none());
    }

    #[test]
    fn stdcell_library_generic() {
        let lib = stdcell::create_generic_lib(TechNode::N7);
        assert!(lib.cells.len() >= 20, "expected >=20 cells, got {}", lib.cells.len());
        let invs = lib.find_by_function(&CellFunction::Inv);
        assert!(invs.len() >= 3, "expected >=3 inverters");
        let sorted = lib.cells_sorted_by_area();
        for w in sorted.windows(2) {
            assert!(w[0].area <= w[1].area, "cells not sorted by area");
        }
    }

    #[test]
    fn memory_compiler_sram() {
        let spec = memory::MemorySpec {
            mem_type: memory::MemoryType::Sram,
            words: 1024,
            bits: 32,
            mux: 4,
            banks: 1,
        };
        let result = memory::compile_memory(&spec, TechNode::N7);
        assert!(result.area_um2 > 0.0, "area should be positive");
        assert!(result.read_time_ns > 0.0, "read time should be positive");
        assert!(result.write_time_ns > result.read_time_ns * 0.9, "write >= 0.9*read for SRAM");
        assert!(result.name.starts_with("SRAM_1024x32"));
        // Check address pins: 1024 words = 10 address bits.
        let addr_count = result.pins.iter().filter(|p| p.starts_with("A[")).count();
        assert_eq!(addr_count, 10, "expected 10 address bits for 1024 words");
    }

    #[test]
    fn lef_parse_basic() {
        let lef_str = r#"
MACRO INV_X1
  CLASS CORE ;
  SIZE 0.8 BY 1.4 ;
  SYMMETRY X Y ;
  SITE core_site ;
  PIN A
    DIRECTION INPUT ;
    PORT
      LAYER metal1 ;
        RECT 0.0 0.0 0.1 0.4 ;
    END
  END A
  PIN Y
    DIRECTION OUTPUT ;
    PORT
      LAYER metal1 ;
        RECT 0.6 0.0 0.8 0.4 ;
    END
  END Y
  OBS
    LAYER metal1 ;
      RECT 0.2 0.0 0.6 1.4 ;
  END
END INV_X1
"#;
        let lib = lef::parse_lef(lef_str);
        assert_eq!(lib.macros.len(), 1);
        let m = &lib.macros[0];
        assert_eq!(m.name, "INV_X1");
        assert_eq!(m.class, MacroClass::Core);
        assert!((m.size.0 - 0.8).abs() < 1e-6);
        assert!((m.size.1 - 1.4).abs() < 1e-6);
        assert_eq!(m.pins.len(), 2);
        assert_eq!(m.obs.len(), 1);
    }

    #[test]
    fn memory_compiler_regfile() {
        let spec = memory::MemorySpec {
            mem_type: memory::MemoryType::RegFile,
            words: 32,
            bits: 64,
            mux: 1,
            banks: 1,
        };
        let result = memory::compile_memory(&spec, TechNode::N5);
        assert!(result.area_um2 > 0.0);
        assert!(result.name.starts_with("RF_32x64"));
    }
}
