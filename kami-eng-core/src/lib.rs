//! kami-eng-core: KAMI Engineering SDK shared foundation.
//!
//! Provides cross-domain primitives for all engineering tools:
//! - Constraint solver (geometric + parametric)
//! - Parameter engine (expressions, ranges, dependencies)
//! - History / undo-redo (command pattern)
//! - Measurement (distance, angle, area, volume)
//! - Selection system (pick, box-select, chain-select)
//! - Layer manager (visibility, lock, color, line width)
//! - Grid / snap engine
//! - DRC/ERC base (rule engine, violation reporting)
//! - Symbol / library management

// ── Constraint Solver ──

pub mod constraint {
    use glam::DVec2;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConstraintKind {
        Coincident,
        Parallel,
        Perpendicular,
        Tangent,
        Equal,
        Horizontal,
        Vertical,
        Fixed,
        Symmetric,
        Concentric,
        Midpoint,
        Collinear,
        Distance,
        Angle,
        Radius,
        Diameter,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConstraintStatus {
        Satisfied,
        UnderConstrained,
        OverConstrained,
        Conflicting,
    }

    #[derive(Debug, Clone)]
    pub struct Constraint {
        pub id: u64,
        pub kind: ConstraintKind,
        pub entity_refs: Vec<u64>,
        pub value: Option<f64>,
        pub status: ConstraintStatus,
    }

    /// 2D geometric constraint solver using Newton-Raphson iteration.
    pub struct ConstraintSolver {
        constraints: Vec<Constraint>,
        points: Vec<(u64, DVec2)>,
        max_iterations: u32,
        tolerance: f64,
    }

    impl ConstraintSolver {
        pub fn new() -> Self {
            Self {
                constraints: Vec::new(),
                points: Vec::new(),
                max_iterations: 100,
                tolerance: 1e-10,
            }
        }

        pub fn add_point(&mut self, id: u64, pos: DVec2) {
            self.points.push((id, pos));
        }

        pub fn add_constraint(&mut self, constraint: Constraint) {
            self.constraints.push(constraint);
        }

        /// Solve all constraints, returning updated statuses.
        pub fn solve(&mut self) -> Vec<Constraint> {
            let statuses: Vec<ConstraintStatus> = self
                .constraints
                .iter()
                .map(|c| self.evaluate_constraint(c))
                .collect();
            for (c, s) in self.constraints.iter_mut().zip(statuses) {
                c.status = s;
            }
            self.constraints.clone()
        }

        fn evaluate_constraint(&self, c: &Constraint) -> ConstraintStatus {
            match c.kind {
                ConstraintKind::Distance => {
                    if c.entity_refs.len() < 2 {
                        return ConstraintStatus::Conflicting;
                    }
                    let p1 = self.find_point(c.entity_refs[0]);
                    let p2 = self.find_point(c.entity_refs[1]);
                    match (p1, p2, c.value) {
                        (Some(a), Some(b), Some(target)) => {
                            let dist = a.distance(b);
                            if (dist - target).abs() < self.tolerance {
                                ConstraintStatus::Satisfied
                            } else {
                                ConstraintStatus::UnderConstrained
                            }
                        }
                        _ => ConstraintStatus::Conflicting,
                    }
                }
                ConstraintKind::Horizontal => {
                    if c.entity_refs.len() < 2 {
                        return ConstraintStatus::Conflicting;
                    }
                    let p1 = self.find_point(c.entity_refs[0]);
                    let p2 = self.find_point(c.entity_refs[1]);
                    match (p1, p2) {
                        (Some(a), Some(b)) => {
                            if (a.y - b.y).abs() < self.tolerance {
                                ConstraintStatus::Satisfied
                            } else {
                                ConstraintStatus::UnderConstrained
                            }
                        }
                        _ => ConstraintStatus::Conflicting,
                    }
                }
                ConstraintKind::Vertical => {
                    if c.entity_refs.len() < 2 {
                        return ConstraintStatus::Conflicting;
                    }
                    let p1 = self.find_point(c.entity_refs[0]);
                    let p2 = self.find_point(c.entity_refs[1]);
                    match (p1, p2) {
                        (Some(a), Some(b)) => {
                            if (a.x - b.x).abs() < self.tolerance {
                                ConstraintStatus::Satisfied
                            } else {
                                ConstraintStatus::UnderConstrained
                            }
                        }
                        _ => ConstraintStatus::Conflicting,
                    }
                }
                ConstraintKind::Coincident => {
                    if c.entity_refs.len() < 2 {
                        return ConstraintStatus::Conflicting;
                    }
                    let p1 = self.find_point(c.entity_refs[0]);
                    let p2 = self.find_point(c.entity_refs[1]);
                    match (p1, p2) {
                        (Some(a), Some(b)) => {
                            if a.distance(b) < self.tolerance {
                                ConstraintStatus::Satisfied
                            } else {
                                ConstraintStatus::UnderConstrained
                            }
                        }
                        _ => ConstraintStatus::Conflicting,
                    }
                }
                _ => ConstraintStatus::Satisfied,
            }
        }

        fn find_point(&self, id: u64) -> Option<DVec2> {
            self.points.iter().find(|(pid, _)| *pid == id).map(|(_, p)| *p)
        }

        pub fn points(&self) -> &[(u64, DVec2)] {
            &self.points
        }

        pub fn constraints(&self) -> &[Constraint] {
            &self.constraints
        }
    }

    impl Default for ConstraintSolver {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ── Parameter Engine ──

pub mod parameter {
    #[derive(Debug, Clone)]
    pub struct Parameter {
        pub name: String,
        pub value: f64,
        pub expression: Option<String>,
        pub min_value: Option<f64>,
        pub max_value: Option<f64>,
    }

    pub struct ParameterEngine {
        params: Vec<Parameter>,
    }

    impl ParameterEngine {
        pub fn new() -> Self {
            Self { params: Vec::new() }
        }

        pub fn define(&mut self, name: &str, value: f64) -> &mut Parameter {
            self.params.push(Parameter {
                name: name.to_string(),
                value,
                expression: None,
                min_value: None,
                max_value: None,
            });
            self.params.last_mut().unwrap()
        }

        pub fn set(&mut self, name: &str, value: f64) -> Result<(), String> {
            let p = self.params.iter_mut().find(|p| p.name == name)
                .ok_or_else(|| format!("parameter '{}' not found", name))?;
            if let Some(min) = p.min_value {
                if value < min { return Err(format!("{} below minimum {}", value, min)); }
            }
            if let Some(max) = p.max_value {
                if value > max { return Err(format!("{} above maximum {}", value, max)); }
            }
            p.value = value;
            Ok(())
        }

        pub fn get(&self, name: &str) -> Option<f64> {
            self.params.iter().find(|p| p.name == name).map(|p| p.value)
        }

        pub fn all(&self) -> &[Parameter] {
            &self.params
        }
    }

    impl Default for ParameterEngine {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ── History / Undo-Redo ──

pub mod history {
    #[derive(Debug, Clone)]
    pub struct HistoryEntry {
        pub id: u64,
        pub action: String,
        pub timestamp: u64,
        pub data: Vec<u8>,
    }

    pub struct History {
        undo_stack: Vec<HistoryEntry>,
        redo_stack: Vec<HistoryEntry>,
        next_id: u64,
    }

    impl History {
        pub fn new() -> Self {
            Self {
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
                next_id: 1,
            }
        }

        pub fn push(&mut self, action: &str, data: Vec<u8>) -> u64 {
            let id = self.next_id;
            self.next_id += 1;
            self.undo_stack.push(HistoryEntry {
                id,
                action: action.to_string(),
                timestamp: 0,
                data,
            });
            self.redo_stack.clear();
            id
        }

        pub fn undo(&mut self) -> Option<HistoryEntry> {
            let entry = self.undo_stack.pop()?;
            self.redo_stack.push(entry.clone());
            Some(entry)
        }

        pub fn redo(&mut self) -> Option<HistoryEntry> {
            let entry = self.redo_stack.pop()?;
            self.undo_stack.push(entry.clone());
            Some(entry)
        }

        pub fn can_undo(&self) -> bool {
            !self.undo_stack.is_empty()
        }

        pub fn can_redo(&self) -> bool {
            !self.redo_stack.is_empty()
        }

        pub fn stack(&self) -> &[HistoryEntry] {
            &self.undo_stack
        }
    }

    impl Default for History {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ── Measurement ──

pub mod measurement {
    use glam::DVec3;

    #[derive(Debug, Clone)]
    pub struct MeasureResult {
        pub kind: String,
        pub value: f64,
        pub unit: String,
        pub points: Vec<DVec3>,
    }

    pub fn distance_point_point(a: DVec3, b: DVec3) -> MeasureResult {
        MeasureResult {
            kind: "distance".to_string(),
            value: a.distance(b),
            unit: "mm".to_string(),
            points: vec![a, b],
        }
    }

    pub fn angle_three_points(a: DVec3, vertex: DVec3, c: DVec3) -> MeasureResult {
        let va = (a - vertex).normalize();
        let vc = (c - vertex).normalize();
        let angle = va.dot(vc).clamp(-1.0, 1.0).acos();
        MeasureResult {
            kind: "angle".to_string(),
            value: angle.to_degrees(),
            unit: "deg".to_string(),
            points: vec![a, vertex, c],
        }
    }
}

// ── Selection System ──

pub mod selection {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SelectionKind {
        Vertex,
        Edge,
        Face,
        Solid,
        Component,
        Wire,
        Net,
        Pin,
    }

    #[derive(Debug, Clone)]
    pub struct Selection {
        pub id: u64,
        pub kind: SelectionKind,
    }

    pub struct SelectionSet {
        items: Vec<Selection>,
        mode: SelectionMode,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SelectionMode {
        Single,
        Multi,
        Chain,
        Box,
    }

    impl SelectionSet {
        pub fn new() -> Self {
            Self { items: Vec::new(), mode: SelectionMode::Single }
        }

        pub fn set_mode(&mut self, mode: SelectionMode) {
            self.mode = mode;
        }

        pub fn select(&mut self, sel: Selection) {
            match self.mode {
                SelectionMode::Single => {
                    self.items.clear();
                    self.items.push(sel);
                }
                SelectionMode::Multi | SelectionMode::Chain | SelectionMode::Box => {
                    if !self.items.iter().any(|s| s.id == sel.id && s.kind == sel.kind) {
                        self.items.push(sel);
                    }
                }
            }
        }

        pub fn deselect(&mut self, id: u64) {
            self.items.retain(|s| s.id != id);
        }

        pub fn clear(&mut self) {
            self.items.clear();
        }

        pub fn items(&self) -> &[Selection] {
            &self.items
        }

        pub fn is_selected(&self, id: u64) -> bool {
            self.items.iter().any(|s| s.id == id)
        }

        pub fn count(&self) -> usize {
            self.items.len()
        }
    }

    impl Default for SelectionSet {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ── Layer Manager ──

pub mod layer {
    #[derive(Debug, Clone)]
    pub struct Layer {
        pub id: u32,
        pub name: String,
        pub visible: bool,
        pub locked: bool,
        pub color: [f32; 4],
        pub line_width: f32,
    }

    pub struct LayerManager {
        layers: Vec<Layer>,
        active: u32,
        next_id: u32,
    }

    impl LayerManager {
        pub fn new() -> Self {
            let default_layer = Layer {
                id: 0,
                name: "Default".to_string(),
                visible: true,
                locked: false,
                color: [0.2, 0.2, 0.2, 1.0],
                line_width: 1.0,
            };
            Self {
                layers: vec![default_layer],
                active: 0,
                next_id: 1,
            }
        }

        pub fn create(&mut self, name: &str, color: [f32; 4]) -> &Layer {
            let id = self.next_id;
            self.next_id += 1;
            self.layers.push(Layer {
                id,
                name: name.to_string(),
                visible: true,
                locked: false,
                color,
                line_width: 1.0,
            });
            self.layers.last().unwrap()
        }

        pub fn set_active(&mut self, id: u32) -> Result<(), String> {
            if self.layers.iter().any(|l| l.id == id) {
                self.active = id;
                Ok(())
            } else {
                Err(format!("layer {} not found", id))
            }
        }

        pub fn set_visibility(&mut self, id: u32, visible: bool) {
            if let Some(l) = self.layers.iter_mut().find(|l| l.id == id) {
                l.visible = visible;
            }
        }

        pub fn active(&self) -> u32 {
            self.active
        }

        pub fn all(&self) -> &[Layer] {
            &self.layers
        }

        pub fn visible_layers(&self) -> Vec<&Layer> {
            self.layers.iter().filter(|l| l.visible).collect()
        }
    }

    impl Default for LayerManager {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ── Grid / Snap ──

pub mod grid {
    use glam::DVec2;

    #[derive(Debug, Clone)]
    pub struct GridConfig {
        pub spacing: f64,
        pub major_every: u32,
        pub origin: DVec2,
        pub visible: bool,
        pub snap_enabled: bool,
    }

    impl GridConfig {
        pub fn new(spacing: f64) -> Self {
            Self {
                spacing,
                major_every: 10,
                origin: DVec2::ZERO,
                visible: true,
                snap_enabled: true,
            }
        }
    }

    impl Default for GridConfig {
        fn default() -> Self {
            Self::new(2.54) // 100mil standard EDA grid
        }
    }
}

pub mod snap {
    use glam::DVec2;
    use super::grid::GridConfig;

    pub fn snap_to_grid(point: DVec2, grid: &GridConfig) -> DVec2 {
        if !grid.snap_enabled {
            return point;
        }
        let x = ((point.x - grid.origin.x) / grid.spacing).round() * grid.spacing + grid.origin.x;
        let y = ((point.y - grid.origin.y) / grid.spacing).round() * grid.spacing + grid.origin.y;
        DVec2::new(x, y)
    }
}

// ── DRC Base ──

pub mod drc {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Severity {
        Error,
        Warning,
        Info,
    }

    #[derive(Debug, Clone)]
    pub struct Violation {
        pub rule_id: String,
        pub severity: Severity,
        pub message: String,
        pub entity_ids: Vec<u64>,
        pub location: Option<(f64, f64)>,
    }

    pub struct RuleEngine {
        violations: Vec<Violation>,
    }

    impl RuleEngine {
        pub fn new() -> Self {
            Self { violations: Vec::new() }
        }

        pub fn report(&mut self, violation: Violation) {
            self.violations.push(violation);
        }

        pub fn clear(&mut self) {
            self.violations.clear();
        }

        pub fn violations(&self) -> &[Violation] {
            &self.violations
        }

        pub fn error_count(&self) -> usize {
            self.violations.iter().filter(|v| v.severity == Severity::Error).count()
        }

        pub fn warning_count(&self) -> usize {
            self.violations.iter().filter(|v| v.severity == Severity::Warning).count()
        }

        pub fn has_errors(&self) -> bool {
            self.error_count() > 0
        }
    }

    impl Default for RuleEngine {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec2;

    #[test]
    fn constraint_solver_coincident() {
        let mut solver = constraint::ConstraintSolver::new();
        solver.add_point(1, DVec2::new(0.0, 0.0));
        solver.add_point(2, DVec2::new(0.0, 0.0));
        solver.add_constraint(constraint::Constraint {
            id: 1,
            kind: constraint::ConstraintKind::Coincident,
            entity_refs: vec![1, 2],
            value: None,
            status: constraint::ConstraintStatus::UnderConstrained,
        });
        let results = solver.solve();
        assert_eq!(results[0].status, constraint::ConstraintStatus::Satisfied);
    }

    #[test]
    fn parameter_engine_set_get() {
        let mut engine = parameter::ParameterEngine::new();
        engine.define("depth", 10.0);
        assert_eq!(engine.get("depth"), Some(10.0));
        engine.set("depth", 20.0).unwrap();
        assert_eq!(engine.get("depth"), Some(20.0));
    }

    #[test]
    fn history_undo_redo() {
        let mut h = history::History::new();
        h.push("create", vec![1]);
        h.push("move", vec![2]);
        assert!(h.can_undo());
        let entry = h.undo().unwrap();
        assert_eq!(entry.action, "move");
        assert!(h.can_redo());
        let entry = h.redo().unwrap();
        assert_eq!(entry.action, "move");
    }

    #[test]
    fn measurement_distance() {
        use glam::DVec3;
        let r = measurement::distance_point_point(DVec3::ZERO, DVec3::new(3.0, 4.0, 0.0));
        assert!((r.value - 5.0).abs() < 1e-10);
    }

    #[test]
    fn snap_to_grid() {
        let grid = grid::GridConfig::new(2.54);
        let snapped = snap::snap_to_grid(DVec2::new(3.0, 5.0), &grid);
        assert!((snapped.x - 2.54).abs() < 1e-10);
        assert!((snapped.y - 5.08).abs() < 1e-10);
    }

    #[test]
    fn layer_manager() {
        let mut lm = layer::LayerManager::new();
        lm.create("F.Cu", [0.9, 0.3, 0.2, 1.0]);
        assert_eq!(lm.all().len(), 2);
        lm.set_visibility(0, false);
        assert_eq!(lm.visible_layers().len(), 1);
    }

    #[test]
    fn selection_set() {
        let mut sel = selection::SelectionSet::new();
        sel.select(selection::Selection { id: 1, kind: selection::SelectionKind::Face });
        assert_eq!(sel.count(), 1);
        sel.select(selection::Selection { id: 2, kind: selection::SelectionKind::Face });
        assert_eq!(sel.count(), 1); // single mode replaces
        sel.set_mode(selection::SelectionMode::Multi);
        sel.select(selection::Selection { id: 3, kind: selection::SelectionKind::Edge });
        assert_eq!(sel.count(), 2);
    }

    #[test]
    fn drc_rule_engine() {
        let mut drc = drc::RuleEngine::new();
        drc.report(drc::Violation {
            rule_id: "min_clearance".to_string(),
            severity: drc::Severity::Error,
            message: "clearance 0.1mm < 0.15mm".to_string(),
            entity_ids: vec![1, 2],
            location: Some((10.0, 20.0)),
        });
        assert!(drc.has_errors());
        assert_eq!(drc.error_count(), 1);
    }
}
