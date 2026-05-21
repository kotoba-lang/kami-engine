//! # kami-eda — Electronic Design Automation
//!
//! Schematic capture, PCB layout, netlist/BOM generation, and ERC/DRC
//! validation for the KAMI engine. Positions use `glam::Vec2`. Violations
//! are reported through `kami_eng_core::drc::Violation` for unified
//! rule-engine integration.
//!
//! ## Optional integration
//!
//! `kami-graph` is available as a path dependency for visualizing
//! connectivity graphs (force-directed net topology, Merkle DAG PCB
//! layer stacks) but is not directly invoked in the core EDA logic.

// ── Shared types ──

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Unique identifier for an EDA entity (symbol, wire, footprint, etc.).
pub type EntityId = u64;

/// Unique identifier for a net (electrical connection).
pub type NetId = u64;

/// Cardinal orientation for pins and labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Left,
    Right,
    Up,
    Down,
}

/// Copper layer identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Layer {
    Front,
    Back,
    Inner(u8),
}

// ────────────────────────────────────────────────────────────────────
// schematic module
// ────────────────────────────────────────────────────────────────────

pub mod schematic {
    //! Schematic capture: symbol placement, wire routing, net labelling,
    //! and netlist extraction.

    use super::*;
    use std::collections::HashMap;

    /// Electrical function of a pin.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum PinType {
        Input,
        Output,
        Bidirectional,
        TriState,
        Passive,
        Power,
        OpenCollector,
        OpenEmitter,
        NotConnected,
        Unspecified,
    }

    /// A pin on a schematic symbol.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Pin {
        pub name: String,
        pub number: String,
        pub pin_type: PinType,
        pub position: Vec2,
        pub orientation: Orientation,
    }

    /// An instance of a library symbol placed on a schematic sheet.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SymbolInstance {
        pub id: EntityId,
        pub library_ref: String,
        pub designator: String,
        pub value: String,
        pub position: Vec2,
        pub rotation: f32,
        pub mirror: bool,
        pub pins: Vec<Pin>,
    }

    /// A wire segment connecting two points on the schematic.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Wire {
        pub id: EntityId,
        pub start: Vec2,
        pub end: Vec2,
        pub net_id: Option<NetId>,
    }

    /// An electrical net grouping connected pins.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Net {
        pub id: NetId,
        pub name: String,
        /// (symbol entity id, pin number)
        pub pins: Vec<(EntityId, String)>,
    }

    /// A label that assigns a net name at a point on the schematic.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct NetLabel {
        pub id: EntityId,
        pub name: String,
        pub position: Vec2,
        pub orientation: Orientation,
    }

    /// A power-port symbol (VCC, GND, etc.).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PowerPort {
        pub id: EntityId,
        pub name: String,
        pub position: Vec2,
        pub orientation: Orientation,
    }

    /// A junction that explicitly merges crossing wires.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Junction {
        pub id: EntityId,
        pub position: Vec2,
    }

    /// A single schematic sheet.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SchematicSheet {
        pub name: String,
        pub width: f32,
        pub height: f32,
    }

    /// Top-level schematic container.
    #[derive(Debug, Clone, Default)]
    pub struct Schematic {
        next_id: EntityId,
        pub sheets: Vec<SchematicSheet>,
        pub symbols: Vec<SymbolInstance>,
        pub wires: Vec<Wire>,
        pub nets: HashMap<NetId, Net>,
        pub labels: Vec<NetLabel>,
        pub power_ports: Vec<PowerPort>,
        pub junctions: Vec<Junction>,
    }

    impl Schematic {
        pub fn new() -> Self {
            Self::default()
        }

        fn alloc_id(&mut self) -> EntityId {
            self.next_id += 1;
            self.next_id
        }

        /// Place a symbol instance on the schematic. Returns the assigned
        /// entity id.
        pub fn place_symbol(
            &mut self,
            library_ref: &str,
            designator: &str,
            value: &str,
            position: Vec2,
            rotation: f32,
            mirror: bool,
            pins: Vec<Pin>,
        ) -> EntityId {
            let id = self.alloc_id();
            self.symbols.push(SymbolInstance {
                id,
                library_ref: library_ref.to_string(),
                designator: designator.to_string(),
                value: value.to_string(),
                position,
                rotation,
                mirror,
                pins,
            });
            id
        }

        /// Route a wire between two points, optionally assigning it to a net.
        pub fn route_wire(&mut self, start: Vec2, end: Vec2, net_id: Option<NetId>) -> EntityId {
            let id = self.alloc_id();
            self.wires.push(Wire { id, start, end, net_id });
            id
        }

        /// Add a net label at the given position.
        pub fn add_net_label(
            &mut self,
            name: &str,
            position: Vec2,
            orientation: Orientation,
        ) -> EntityId {
            let id = self.alloc_id();
            self.labels.push(NetLabel {
                id,
                name: name.to_string(),
                position,
                orientation,
            });
            id
        }

        /// Build the netlist from placed symbols and wires. Returns a map
        /// of `NetId` → `Net`. Pins sharing the same `net_id` through
        /// wires are grouped into the same net.
        pub fn generate_netlist(&self) -> HashMap<NetId, Net> {
            let mut nets: HashMap<NetId, Net> = HashMap::new();

            // Walk every symbol. For each pin whose position coincides with
            // a wire endpoint, inherit the wire's net_id.
            for sym in &self.symbols {
                for pin in &sym.pins {
                    let abs_pin_pos = sym.position + pin.position;
                    for wire in &self.wires {
                        if let Some(nid) = wire.net_id {
                            let eps = 0.01;
                            if abs_pin_pos.distance(wire.start) < eps
                                || abs_pin_pos.distance(wire.end) < eps
                            {
                                let net = nets.entry(nid).or_insert_with(|| Net {
                                    id: nid,
                                    name: self
                                        .labels
                                        .iter()
                                        .find(|l| {
                                            l.position.distance(wire.start) < eps
                                                || l.position.distance(wire.end) < eps
                                        })
                                        .map(|l| l.name.clone())
                                        .unwrap_or_else(|| format!("NET_{nid}")),
                                    pins: Vec::new(),
                                });
                                let entry = (sym.id, pin.number.clone());
                                if !net.pins.contains(&entry) {
                                    net.pins.push(entry);
                                }
                            }
                        }
                    }
                }
            }

            nets
        }

        /// Delete any entity (symbol, wire, label, power port, junction)
        /// by id.
        pub fn delete_entity(&mut self, id: EntityId) -> bool {
            let before = self.symbols.len()
                + self.wires.len()
                + self.labels.len()
                + self.power_ports.len()
                + self.junctions.len();

            self.symbols.retain(|s| s.id != id);
            self.wires.retain(|w| w.id != id);
            self.labels.retain(|l| l.id != id);
            self.power_ports.retain(|p| p.id != id);
            self.junctions.retain(|j| j.id != id);

            let after = self.symbols.len()
                + self.wires.len()
                + self.labels.len()
                + self.power_ports.len()
                + self.junctions.len();

            after < before
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// pcb module
// ────────────────────────────────────────────────────────────────────

pub mod pcb {
    //! PCB layout: footprint placement, trace routing, via insertion,
    //! copper zone pours, and design-rule checking.

    use super::*;
    use kami_eng_core::drc;

    /// Pad shape.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum PadShape {
        Round,
        Rect,
        Oblong,
        Custom,
    }

    /// Zone fill style.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ZoneFill {
        Solid,
        Hatched,
    }

    /// A pad within a footprint.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Pad {
        pub center: Vec2,
        pub size: Vec2,
        pub shape: PadShape,
        /// Drill diameter; `None` for SMD pads.
        pub drill: Option<f32>,
        pub layers: Vec<Layer>,
    }

    /// A component footprint placed on the PCB.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Footprint {
        pub id: EntityId,
        pub library_ref: String,
        pub designator: String,
        pub x: f32,
        pub y: f32,
        pub rotation: f32,
        pub layer: Layer,
        pub pads: Vec<Pad>,
    }

    /// A copper trace segment.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Trace {
        pub id: EntityId,
        pub net_id: NetId,
        pub points: Vec<Vec2>,
        pub width: f32,
        pub layer: Layer,
    }

    /// A plated through-hole via.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Via {
        pub id: EntityId,
        pub x: f32,
        pub y: f32,
        pub drill: f32,
        pub outer_diameter: f32,
        pub net_id: NetId,
    }

    /// A copper zone (fill or hatched).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Zone {
        pub id: EntityId,
        pub net_id: NetId,
        pub boundary: Vec<Vec2>,
        pub layer: Layer,
        pub fill: ZoneFill,
    }

    /// Layer stack entry.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LayerDef {
        pub layer: Layer,
        pub name: String,
        pub thickness_mm: f32,
    }

    /// Board outline and layer stack definition.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PcbBoard {
        pub outline: Vec<Vec2>,
        pub layer_stack: Vec<LayerDef>,
    }

    /// Design-rule configuration for DRC.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DrcConfig {
        /// Minimum trace width in mm.
        pub min_trace_width: f32,
        /// Minimum clearance between copper features in mm.
        pub min_clearance: f32,
        /// Minimum via drill diameter in mm.
        pub min_via_drill: f32,
        /// Minimum via annular ring in mm.
        pub min_annular_ring: f32,
        /// Minimum drill-to-drill distance in mm.
        pub min_drill_to_drill: f32,
        /// Minimum copper-to-edge clearance in mm.
        pub min_copper_to_edge: f32,
        /// Minimum silk-to-pad clearance in mm.
        pub min_silk_to_pad: f32,
    }

    impl Default for DrcConfig {
        fn default() -> Self {
            Self {
                min_trace_width: 0.15,
                min_clearance: 0.15,
                min_via_drill: 0.2,
                min_annular_ring: 0.125,
                min_drill_to_drill: 0.25,
                min_copper_to_edge: 0.25,
                min_silk_to_pad: 0.05,
            }
        }
    }

    /// Top-level PCB layout container.
    #[derive(Debug, Clone)]
    pub struct PcbLayout {
        next_id: EntityId,
        pub board: PcbBoard,
        pub footprints: Vec<Footprint>,
        pub traces: Vec<Trace>,
        pub vias: Vec<Via>,
        pub zones: Vec<Zone>,
        pub drc_config: DrcConfig,
    }

    impl PcbLayout {
        pub fn new(board: PcbBoard) -> Self {
            Self {
                next_id: 0,
                board,
                footprints: Vec::new(),
                traces: Vec::new(),
                vias: Vec::new(),
                zones: Vec::new(),
                drc_config: DrcConfig::default(),
            }
        }

        fn alloc_id(&mut self) -> EntityId {
            self.next_id += 1;
            self.next_id
        }

        /// Place a footprint on the board. Returns the assigned entity id.
        pub fn place_footprint(
            &mut self,
            library_ref: &str,
            designator: &str,
            x: f32,
            y: f32,
            rotation: f32,
            layer: Layer,
            pads: Vec<Pad>,
        ) -> EntityId {
            let id = self.alloc_id();
            self.footprints.push(Footprint {
                id,
                library_ref: library_ref.to_string(),
                designator: designator.to_string(),
                x,
                y,
                rotation,
                layer,
                pads,
            });
            id
        }

        /// Route a trace along a sequence of waypoints.
        pub fn route_trace(
            &mut self,
            net_id: NetId,
            points: Vec<Vec2>,
            width: f32,
            layer: Layer,
        ) -> EntityId {
            let id = self.alloc_id();
            self.traces.push(Trace { id, net_id, points, width, layer });
            id
        }

        /// Insert a via at the given position.
        pub fn add_via(
            &mut self,
            x: f32,
            y: f32,
            drill: f32,
            outer_diameter: f32,
            net_id: NetId,
        ) -> EntityId {
            let id = self.alloc_id();
            self.vias.push(Via { id, x, y, drill, outer_diameter, net_id });
            id
        }

        /// Pour a copper zone.
        pub fn pour_zone(
            &mut self,
            net_id: NetId,
            boundary: Vec<Vec2>,
            layer: Layer,
            fill: ZoneFill,
        ) -> EntityId {
            let id = self.alloc_id();
            self.zones.push(Zone { id, net_id, boundary, layer, fill });
            id
        }

        /// Run design-rule checks against the current layout.
        /// Returns a list of violations from `kami_eng_core::drc`.
        pub fn run_drc(&self) -> Vec<drc::Violation> {
            let mut violations = Vec::new();
            let cfg = &self.drc_config;

            // Check trace widths.
            for trace in &self.traces {
                if trace.width < cfg.min_trace_width {
                    violations.push(drc::Violation {
                        rule_id: "PCB_MIN_TRACE_WIDTH".to_string(),
                        severity: drc::Severity::Error,
                        message: format!(
                            "Trace (id={}) width {:.3} mm < min {:.3} mm",
                            trace.id, trace.width, cfg.min_trace_width
                        ),
                        entity_ids: vec![trace.id],
                        location: trace.points.first().map(|p| (p.x as f64, p.y as f64)),
                    });
                }
            }

            // Check via drill diameter.
            for via in &self.vias {
                if via.drill < cfg.min_via_drill {
                    violations.push(drc::Violation {
                        rule_id: "PCB_MIN_VIA_DRILL".to_string(),
                        severity: drc::Severity::Error,
                        message: format!(
                            "Via (id={}) drill {:.3} mm < min {:.3} mm",
                            via.id, via.drill, cfg.min_via_drill
                        ),
                        entity_ids: vec![via.id],
                        location: Some((via.x as f64, via.y as f64)),
                    });
                }

                let annular = (via.outer_diameter - via.drill) / 2.0;
                if annular < cfg.min_annular_ring {
                    violations.push(drc::Violation {
                        rule_id: "PCB_MIN_ANNULAR_RING".to_string(),
                        severity: drc::Severity::Error,
                        message: format!(
                            "Via (id={}) annular ring {:.3} mm < min {:.3} mm",
                            via.id, annular, cfg.min_annular_ring
                        ),
                        entity_ids: vec![via.id],
                        location: Some((via.x as f64, via.y as f64)),
                    });
                }
            }

            // Check trace-to-trace clearance on the same layer.
            for (i, a) in self.traces.iter().enumerate() {
                for b in self.traces.iter().skip(i + 1) {
                    if a.layer != b.layer || a.net_id == b.net_id {
                        continue;
                    }
                    for pa in &a.points {
                        for pb in &b.points {
                            let dist = pa.distance(*pb);
                            let required = cfg.min_clearance + (a.width + b.width) / 2.0;
                            if dist < required {
                                violations.push(drc::Violation {
                                    rule_id: "PCB_MIN_CLEARANCE".to_string(),
                                    severity: drc::Severity::Error,
                                    message: format!(
                                        "Clearance {:.3} mm between traces (id={}, id={}) < min {:.3} mm",
                                        dist, a.id, b.id, cfg.min_clearance
                                    ),
                                    entity_ids: vec![a.id, b.id],
                                    location: Some((pa.x as f64, pa.y as f64)),
                                });
                            }
                        }
                    }
                }
            }

            violations
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// netlist module
// ────────────────────────────────────────────────────────────────────

pub mod netlist {
    //! Netlist export (SPICE, Verilog gate-level, EDIF) and BOM generation.

    use super::schematic::{Net, SymbolInstance};
    use std::collections::HashMap;
    use std::fmt::Write;

    /// Supported netlist output formats.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NetlistFormat {
        Spice,
        VerilogGateLevel,
        Edif,
    }

    /// A single BOM line item.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BomEntry {
        pub designator: String,
        pub value: String,
        pub footprint: String,
        pub quantity: u32,
    }

    /// Generate a bill of materials from the placed symbols. Symbols
    /// sharing the same (value, library_ref) are grouped and counted.
    pub fn generate_bom(symbols: &[SymbolInstance]) -> Vec<BomEntry> {
        let mut groups: HashMap<(String, String), (String, u32)> = HashMap::new();
        let mut designators: HashMap<(String, String), Vec<String>> = HashMap::new();

        for sym in symbols {
            let key = (sym.value.clone(), sym.library_ref.clone());
            groups
                .entry(key.clone())
                .and_modify(|(_, q)| *q += 1)
                .or_insert((sym.library_ref.clone(), 1));
            designators.entry(key).or_default().push(sym.designator.clone());
        }

        let mut bom: Vec<BomEntry> = groups
            .into_iter()
            .map(|((value, _), (footprint, quantity))| {
                let key = (value.clone(), footprint.clone());
                let desigs = designators.get(&key).cloned().unwrap_or_default();
                BomEntry {
                    designator: desigs.join(", "),
                    value,
                    footprint,
                    quantity,
                }
            })
            .collect();

        bom.sort_by(|a, b| a.designator.cmp(&b.designator));
        bom
    }

    /// SPICE netlist exporter.
    pub struct SpiceExporter<'a> {
        pub symbols: &'a [SymbolInstance],
        pub nets: &'a HashMap<super::NetId, Net>,
    }

    impl<'a> SpiceExporter<'a> {
        /// Render the SPICE netlist as a string.
        pub fn to_string(&self) -> String {
            let mut out = String::from("* KAMI EDA — SPICE netlist\n");

            for sym in self.symbols {
                let _ = write!(out, "{} ", sym.designator);
                // List connected net names for each pin in order.
                for pin in &sym.pins {
                    let net_name = self
                        .nets
                        .values()
                        .find(|n| n.pins.iter().any(|(sid, pn)| *sid == sym.id && *pn == pin.number))
                        .map(|n| n.name.as_str())
                        .unwrap_or("?");
                    let _ = write!(out, "{net_name} ");
                }
                let _ = writeln!(out, "{}", sym.value);
            }

            let _ = writeln!(out, ".end");
            out
        }
    }

    /// Export a netlist in the requested format. Currently only SPICE is
    /// fully implemented; other formats return a stub header.
    pub fn export_netlist(
        format: NetlistFormat,
        symbols: &[SymbolInstance],
        nets: &HashMap<super::NetId, Net>,
    ) -> String {
        match format {
            NetlistFormat::Spice => {
                let exporter = SpiceExporter { symbols, nets };
                exporter.to_string()
            }
            NetlistFormat::VerilogGateLevel => {
                let mut out = String::from("// KAMI EDA — Verilog gate-level netlist\n");
                let _ = writeln!(out, "module top;");
                for net in nets.values() {
                    let _ = writeln!(out, "  wire {};", net.name);
                }
                let _ = writeln!(out, "endmodule");
                out
            }
            NetlistFormat::Edif => {
                let mut out = String::from("(edif kami_eda\n");
                let _ = writeln!(out, "  (edifVersion 2 0 0)");
                let _ = writeln!(out, "  (edifLevel 0)");
                let _ = writeln!(out, "  (keywordMap (keywordLevel 0))");
                for sym in symbols {
                    let _ = writeln!(out, "  (instance {} (viewRef {}))", sym.designator, sym.library_ref);
                }
                let _ = writeln!(out, ")");
                out
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// erc module
// ────────────────────────────────────────────────────────────────────

pub mod erc {
    //! Electrical Rule Check (ERC) for schematic validation.

    use super::schematic::{PinType, Schematic};
    use kami_eng_core::drc;
    use std::collections::HashMap;

    /// Run all ERC rules on the schematic and return a list of violations.
    pub fn run_erc(schematic: &Schematic) -> Vec<drc::Violation> {
        let mut violations = Vec::new();
        check_unconnected_pins(schematic, &mut violations);
        check_output_to_output(schematic, &mut violations);
        check_power_undriven(schematic, &mut violations);
        check_net_single_pin(schematic, &mut violations);
        check_duplicate_designator(schematic, &mut violations);
        violations
    }

    /// ERC-001: Pins that are not connected to any wire.
    fn check_unconnected_pins(sch: &Schematic, out: &mut Vec<drc::Violation>) {
        let eps = 0.01_f32;
        for sym in &sch.symbols {
            for pin in &sym.pins {
                if pin.pin_type == PinType::NotConnected {
                    continue;
                }
                let abs_pos = sym.position + pin.position;
                let connected = sch.wires.iter().any(|w| {
                    abs_pos.distance(w.start) < eps || abs_pos.distance(w.end) < eps
                });
                if !connected {
                    out.push(drc::Violation {
                        rule_id: "ERC_UNCONNECTED_PIN".to_string(),
                        severity: drc::Severity::Warning,
                        message: format!(
                            "Pin {} ({}) of {} is unconnected",
                            pin.number, pin.name, sym.designator
                        ),
                        entity_ids: vec![sym.id],
                        location: Some((abs_pos.x as f64, abs_pos.y as f64)),
                    });
                }
            }
        }
    }

    /// ERC-002: Two output pins on the same net (driver conflict).
    fn check_output_to_output(sch: &Schematic, out: &mut Vec<drc::Violation>) {
        let netlist = sch.generate_netlist();
        for net in netlist.values() {
            let outputs: Vec<_> = net
                .pins
                .iter()
                .filter(|(sym_id, pin_num)| {
                    sch.symbols.iter().any(|s| {
                        s.id == *sym_id
                            && s.pins
                                .iter()
                                .any(|p| p.number == *pin_num && p.pin_type == PinType::Output)
                    })
                })
                .collect();

            if outputs.len() > 1 {
                let ids: Vec<u64> = outputs.iter().map(|(id, _)| *id).collect();
                out.push(drc::Violation {
                    rule_id: "ERC_OUTPUT_CONFLICT".to_string(),
                    severity: drc::Severity::Error,
                    message: format!(
                        "Net '{}' has {} output drivers — potential contention",
                        net.name,
                        outputs.len()
                    ),
                    entity_ids: ids,
                    location: None,
                });
            }
        }
    }

    /// ERC-003: Power net with no driving source.
    fn check_power_undriven(sch: &Schematic, out: &mut Vec<drc::Violation>) {
        let netlist = sch.generate_netlist();
        for net in netlist.values() {
            let has_power_pin = net.pins.iter().any(|(sym_id, pin_num)| {
                sch.symbols.iter().any(|s| {
                    s.id == *sym_id
                        && s.pins
                            .iter()
                            .any(|p| p.number == *pin_num && p.pin_type == PinType::Power)
                })
            });

            if !has_power_pin {
                continue;
            }

            let has_driver = net.pins.iter().any(|(sym_id, pin_num)| {
                sch.symbols.iter().any(|s| {
                    s.id == *sym_id
                        && s.pins.iter().any(|p| {
                            p.number == *pin_num
                                && matches!(p.pin_type, PinType::Output | PinType::Power)
                        })
                })
            });

            if !has_driver {
                out.push(drc::Violation {
                    rule_id: "ERC_POWER_UNDRIVEN".to_string(),
                    severity: drc::Severity::Error,
                    message: format!("Power net '{}' has no driving source", net.name),
                    entity_ids: vec![],
                    location: None,
                });
            }
        }
    }

    /// ERC-004: Nets with only a single pin (likely incomplete wiring).
    fn check_net_single_pin(sch: &Schematic, out: &mut Vec<drc::Violation>) {
        let netlist = sch.generate_netlist();
        for net in netlist.values() {
            if net.pins.len() == 1 {
                out.push(drc::Violation {
                    rule_id: "ERC_NET_SINGLE_PIN".to_string(),
                    severity: drc::Severity::Warning,
                    message: format!("Net '{}' has only one pin connected", net.name),
                    entity_ids: net.pins.iter().map(|(id, _)| *id).collect(),
                    location: None,
                });
            }
        }
    }

    /// ERC-005: Duplicate reference designators (e.g. two components both
    /// called "R1").
    fn check_duplicate_designator(sch: &Schematic, out: &mut Vec<drc::Violation>) {
        let mut seen: HashMap<&str, Vec<u64>> = HashMap::new();
        for sym in &sch.symbols {
            seen.entry(&sym.designator).or_default().push(sym.id);
        }
        for (desig, ids) in &seen {
            if ids.len() > 1 {
                out.push(drc::Violation {
                    rule_id: "ERC_DUPLICATE_DESIGNATOR".to_string(),
                    severity: drc::Severity::Error,
                    message: format!(
                        "Designator '{}' used by {} symbols",
                        desig,
                        ids.len()
                    ),
                    entity_ids: ids.clone(),
                    location: None,
                });
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    #[test]
    fn schematic_place_and_delete() {
        let mut sch = schematic::Schematic::new();
        let id = sch.place_symbol("R_0402", "R1", "10k", Vec2::ZERO, 0.0, false, vec![]);
        assert_eq!(sch.symbols.len(), 1);
        assert!(sch.delete_entity(id));
        assert!(sch.symbols.is_empty());
    }

    #[test]
    fn schematic_generate_netlist() {
        let mut sch = schematic::Schematic::new();
        let pin_a = schematic::Pin {
            name: "1".into(),
            number: "1".into(),
            pin_type: schematic::PinType::Passive,
            position: Vec2::new(1.0, 0.0),
            orientation: Orientation::Right,
        };
        let pin_b = schematic::Pin {
            name: "1".into(),
            number: "1".into(),
            pin_type: schematic::PinType::Passive,
            position: Vec2::new(-1.0, 0.0),
            orientation: Orientation::Left,
        };
        sch.place_symbol("R_0402", "R1", "10k", Vec2::new(0.0, 0.0), 0.0, false, vec![pin_a]);
        sch.place_symbol("R_0402", "R2", "4.7k", Vec2::new(4.0, 0.0), 0.0, false, vec![pin_b]);
        sch.route_wire(Vec2::new(1.0, 0.0), Vec2::new(3.0, 0.0), Some(1));

        let netlist = sch.generate_netlist();
        assert_eq!(netlist.len(), 1);
        let net = netlist.get(&1).unwrap();
        assert_eq!(net.pins.len(), 2);
    }

    #[test]
    fn pcb_drc_trace_width() {
        let board = pcb::PcbBoard {
            outline: vec![Vec2::ZERO, Vec2::new(100.0, 0.0), Vec2::new(100.0, 100.0), Vec2::new(0.0, 100.0)],
            layer_stack: vec![pcb::LayerDef { layer: Layer::Front, name: "F.Cu".into(), thickness_mm: 0.035 }],
        };
        let mut layout = pcb::PcbLayout::new(board);
        layout.route_trace(1, vec![Vec2::ZERO, Vec2::new(10.0, 0.0)], 0.05, Layer::Front);

        let violations = layout.run_drc();
        assert!(violations.iter().any(|v| v.rule_id == "PCB_MIN_TRACE_WIDTH"));
    }

    #[test]
    fn pcb_drc_via_drill() {
        let board = pcb::PcbBoard {
            outline: vec![Vec2::ZERO, Vec2::new(50.0, 50.0)],
            layer_stack: vec![],
        };
        let mut layout = pcb::PcbLayout::new(board);
        // Drill too small, annular ring too small.
        layout.add_via(10.0, 10.0, 0.1, 0.3, 1);

        let violations = layout.run_drc();
        assert!(violations.iter().any(|v| v.rule_id == "PCB_MIN_VIA_DRILL"));
        assert!(violations.iter().any(|v| v.rule_id == "PCB_MIN_ANNULAR_RING"));
    }

    #[test]
    fn erc_duplicate_designator() {
        let mut sch = schematic::Schematic::new();
        sch.place_symbol("R_0402", "R1", "10k", Vec2::ZERO, 0.0, false, vec![]);
        sch.place_symbol("C_0402", "R1", "100n", Vec2::new(5.0, 0.0), 0.0, false, vec![]);

        let violations = erc::run_erc(&sch);
        assert!(violations.iter().any(|v| v.rule_id == "ERC_DUPLICATE_DESIGNATOR"));
    }

    #[test]
    fn netlist_bom_generation() {
        let symbols = vec![
            schematic::SymbolInstance {
                id: 1,
                library_ref: "R_0402".into(),
                designator: "R1".into(),
                value: "10k".into(),
                position: Vec2::ZERO,
                rotation: 0.0,
                mirror: false,
                pins: vec![],
            },
            schematic::SymbolInstance {
                id: 2,
                library_ref: "R_0402".into(),
                designator: "R2".into(),
                value: "10k".into(),
                position: Vec2::new(5.0, 0.0),
                rotation: 0.0,
                mirror: false,
                pins: vec![],
            },
        ];
        let bom = netlist::generate_bom(&symbols);
        assert_eq!(bom.len(), 1);
        assert_eq!(bom[0].quantity, 2);
        assert_eq!(bom[0].value, "10k");
    }

    #[test]
    fn netlist_spice_export() {
        use std::collections::HashMap;

        let pin = schematic::Pin {
            name: "1".into(),
            number: "1".into(),
            pin_type: schematic::PinType::Passive,
            position: Vec2::ZERO,
            orientation: Orientation::Right,
        };
        let sym = schematic::SymbolInstance {
            id: 1,
            library_ref: "R_0402".into(),
            designator: "R1".into(),
            value: "10k".into(),
            position: Vec2::ZERO,
            rotation: 0.0,
            mirror: false,
            pins: vec![pin],
        };
        let mut nets = HashMap::new();
        nets.insert(1, schematic::Net {
            id: 1,
            name: "VCC".into(),
            pins: vec![(1, "1".into())],
        });

        let spice = netlist::export_netlist(netlist::NetlistFormat::Spice, &[sym], &nets);
        assert!(spice.contains("R1"));
        assert!(spice.contains("VCC"));
        assert!(spice.contains(".end"));
    }
}
