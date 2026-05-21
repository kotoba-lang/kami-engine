//! kami-devtools: automation and inspection contracts for KAMI runtimes.
//!
//! This crate does not capture screenshots or click real UI by itself.
//! It defines:
//!
//! - semantic element snapshots
//! - automation plans and steps
//! - synthetic input generation
//! - screenshot artifact metadata
//!
//! Host runtimes such as `kami-web` or `magatama-kami-host` can implement the
//! actual screenshot capture and event injection using these shared contracts.

use glam::Vec2;
use kami_input::{Device, InputEvent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SemanticRole {
    Button,
    Panel,
    Meter,
    Text,
    Canvas,
    Node,
    ListItem,
    Input,
    Toggle,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn center(&self) -> Vec2 {
        Vec2::new(self.x + self.width * 0.5, self.y + self.height * 0.5)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementSnapshot {
    pub id: String,
    pub role: SemanticRole,
    pub rect: Rect,
    pub visible: bool,
    pub enabled: bool,
    pub text: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneSnapshot {
    pub width: u32,
    pub height: u32,
    pub elements: Vec<ElementSnapshot>,
}

impl SceneSnapshot {
    pub fn find_by_id(&self, id: &str) -> Option<&ElementSnapshot> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn find_by_tag(&self, tag: &str) -> Option<&ElementSnapshot> {
        self.elements
            .iter()
            .find(|e| e.tags.iter().any(|t| t == tag))
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<&ElementSnapshot> {
        self.elements
            .iter()
            .rev()
            .find(|e| e.visible && e.enabled && e.rect.contains(x, y))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TargetRef {
    ElementId(String),
    Tag(String),
    Position { x: f32, y: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScreenshotFormat {
    Png,
    RawRgba,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenshotArtifact {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub format: ScreenshotFormat,
    pub path: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AutomationStep {
    WaitForElement { target: TargetRef, timeout_ms: u64 },
    Click { target: TargetRef },
    DoubleClick { target: TargetRef },
    MovePointer { target: TargetRef },
    KeyPress { code: String },
    Screenshot { name: String, tags: Vec<String> },
    AssertTextContains { target: TargetRef, needle: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationPlan {
    pub id: String,
    pub steps: Vec<AutomationStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationLogEntry {
    pub step_index: usize,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AutomationTranscript {
    pub plan_id: String,
    pub logs: Vec<AutomationLogEntry>,
    pub screenshots: Vec<ScreenshotArtifact>,
}

impl AutomationTranscript {
    pub fn log(&mut self, step_index: usize, status: impl Into<String>, detail: impl Into<String>) {
        self.logs.push(AutomationLogEntry {
            step_index,
            status: status.into(),
            detail: detail.into(),
        });
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UiUxSeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderCapabilities {
    pub text_visible: bool,
    pub keyboard_navigation: bool,
    pub focus_ring_visible: bool,
    pub hover_feedback: bool,
    pub responsive_layout: bool,
    pub semantic_lists: bool,
}

impl Default for RenderCapabilities {
    fn default() -> Self {
        Self {
            text_visible: true,
            keyboard_navigation: true,
            focus_ring_visible: true,
            hover_feedback: true,
            responsive_layout: true,
            semantic_lists: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiUxFinding {
    pub severity: UiUxSeverity,
    pub rule_id: String,
    pub message: String,
    pub element_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiUxReport {
    pub score: u8,
    pub usable: bool,
    pub findings: Vec<UiUxFinding>,
}

impl UiUxReport {
    pub fn has_blockers(&self) -> bool {
        self.findings
            .iter()
            .any(|f| matches!(f.severity, UiUxSeverity::Critical | UiUxSeverity::High))
    }
}

pub fn resolve_target<'a>(
    scene: &'a SceneSnapshot,
    target: &TargetRef,
) -> Option<&'a ElementSnapshot> {
    match target {
        TargetRef::ElementId(id) => scene.find_by_id(id),
        TargetRef::Tag(tag) => scene.find_by_tag(tag),
        TargetRef::Position { x, y } => scene.hit_test(*x, *y),
    }
}

pub fn click_events_for_target(
    scene: &SceneSnapshot,
    target: &TargetRef,
) -> Option<Vec<InputEvent>> {
    let center = match target {
        TargetRef::Position { x, y } => Vec2::new(*x, *y),
        _ => resolve_target(scene, target)?.rect.center(),
    };
    Some(vec![
        InputEvent::PointerMove {
            x: center.x,
            y: center.y,
            dx: 0.0,
            dy: 0.0,
            device: Device::Mouse,
            stylus: None,
        },
        InputEvent::PointerDown {
            x: center.x,
            y: center.y,
            button: 0,
            device: Device::Mouse,
            stylus: None,
        },
        InputEvent::PointerUp {
            x: center.x,
            y: center.y,
            button: 0,
            device: Device::Mouse,
            stylus: None,
        },
    ])
}

pub fn keypress_events(code: impl Into<String>) -> Vec<InputEvent> {
    let code = code.into();
    vec![
        InputEvent::KeyDown {
            code: code.clone(),
            device: Device::Keyboard,
        },
        InputEvent::KeyUp {
            code,
            device: Device::Keyboard,
        },
    ]
}

pub fn sample_diskcleaner_plan() -> AutomationPlan {
    AutomationPlan {
        id: "diskcleaner-smoke".to_string(),
        steps: vec![
            AutomationStep::WaitForElement {
                target: TargetRef::Tag("hero".to_string()),
                timeout_ms: 5_000,
            },
            AutomationStep::Screenshot {
                name: "boot".to_string(),
                tags: vec!["initial".to_string()],
            },
            AutomationStep::Click {
                target: TargetRef::ElementId("scan-now".to_string()),
            },
            AutomationStep::Screenshot {
                name: "after-click".to_string(),
                tags: vec!["interaction".to_string()],
            },
        ],
    }
}

pub fn evaluate_uiux(scene: &SceneSnapshot, caps: &RenderCapabilities) -> UiUxReport {
    let mut findings = Vec::new();

    let text_elements: Vec<&ElementSnapshot> = scene
        .elements
        .iter()
        .filter(|e| e.role == SemanticRole::Text && e.visible)
        .collect();
    let text_semantics_count = scene
        .elements
        .iter()
        .filter(|e| e.visible && e.text.as_deref().is_some_and(|t| !t.trim().is_empty()))
        .count();
    let interactive: Vec<&ElementSnapshot> = scene
        .elements
        .iter()
        .filter(|e| {
            e.visible
                && e.enabled
                && matches!(
                    e.role,
                    SemanticRole::Button
                        | SemanticRole::Input
                        | SemanticRole::Toggle
                        | SemanticRole::ListItem
                )
        })
        .collect();

    if !caps.text_visible && text_semantics_count > 0 {
        findings.push(UiUxFinding {
            severity: UiUxSeverity::Critical,
            rule_id: "text.not-rendered".to_string(),
            message: "scene contains text semantics, but the renderer cannot display text"
                .to_string(),
            element_id: None,
        });
    }

    for element in &interactive {
        if element.rect.width < 44.0 || element.rect.height < 44.0 {
            findings.push(UiUxFinding {
                severity: UiUxSeverity::High,
                rule_id: "target.too-small".to_string(),
                message: format!(
                    "interactive target is smaller than 44x44 px ({}x{})",
                    element.rect.width, element.rect.height
                ),
                element_id: Some(element.id.clone()),
            });
        }

        let has_own_text = element
            .text
            .as_deref()
            .is_some_and(|t| !t.trim().is_empty());
        let has_embedded_label = text_elements.iter().any(|text| {
            rect_center_distance(&text.rect, &element.rect) < 120.0
                && rect_overlap_ratio(&text.rect, &element.rect) > 0.15
        });
        if !has_own_text && !has_embedded_label {
            findings.push(UiUxFinding {
                severity: UiUxSeverity::High,
                rule_id: "control.unlabeled".to_string(),
                message: "interactive control has no visible or semantic label".to_string(),
                element_id: Some(element.id.clone()),
            });
        }
    }

    if !interactive.is_empty() && !caps.keyboard_navigation {
        findings.push(UiUxFinding {
            severity: UiUxSeverity::High,
            rule_id: "input.keyboard-nav-missing".to_string(),
            message: "interactive UI is present, but keyboard navigation is not supported"
                .to_string(),
            element_id: None,
        });
    }

    if !interactive.is_empty() && !caps.focus_ring_visible {
        findings.push(UiUxFinding {
            severity: UiUxSeverity::Medium,
            rule_id: "focus.not-visible".to_string(),
            message: "interactive UI is present, but focus indication is not visible".to_string(),
            element_id: None,
        });
    }

    if !interactive.is_empty() && !caps.hover_feedback {
        findings.push(UiUxFinding {
            severity: UiUxSeverity::Medium,
            rule_id: "hover.no-feedback".to_string(),
            message: "interactive UI is present, but hover feedback is missing".to_string(),
            element_id: None,
        });
    }

    if scene.width >= 900 && !caps.responsive_layout {
        findings.push(UiUxFinding {
            severity: UiUxSeverity::Medium,
            rule_id: "layout.not-responsive".to_string(),
            message: "layout is fixed-size and does not expose responsive behavior".to_string(),
            element_id: None,
        });
    }

    let max_right = scene
        .elements
        .iter()
        .map(|e| e.rect.x + e.rect.width)
        .fold(0.0, f32::max);
    if max_right > scene.width as f32 {
        findings.push(UiUxFinding {
            severity: UiUxSeverity::High,
            rule_id: "layout.overflow-x".to_string(),
            message: format!(
                "scene content overflows horizontally by {:.1}px",
                max_right - scene.width as f32
            ),
            element_id: None,
        });
    }

    let mut score = 100i32;
    for finding in &findings {
        score -= match finding.severity {
            UiUxSeverity::Critical => 35,
            UiUxSeverity::High => 20,
            UiUxSeverity::Medium => 10,
            UiUxSeverity::Low => 4,
        };
    }
    let score = score.clamp(0, 100) as u8;

    UiUxReport {
        score,
        usable: score >= 70
            && !findings
                .iter()
                .any(|f| f.severity == UiUxSeverity::Critical),
        findings,
    }
}

fn rect_center_distance(a: &Rect, b: &Rect) -> f32 {
    let ac = a.center();
    let bc = b.center();
    ac.distance(bc)
}

fn rect_overlap_ratio(a: &Rect, b: &Rect) -> f32 {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    if right <= left || bottom <= top {
        return 0.0;
    }
    let overlap = (right - left) * (bottom - top);
    let base = (a.width * a.height).min(b.width * b.height).max(1.0);
    overlap / base
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_scene() -> SceneSnapshot {
        SceneSnapshot {
            width: 1280,
            height: 820,
            elements: vec![
                ElementSnapshot {
                    id: "hero".to_string(),
                    role: SemanticRole::Panel,
                    rect: Rect {
                        x: 36.0,
                        y: 36.0,
                        width: 760.0,
                        height: 220.0,
                    },
                    visible: true,
                    enabled: true,
                    text: Some("Disk Cleaner".to_string()),
                    tags: vec!["hero".to_string()],
                },
                ElementSnapshot {
                    id: "scan-now".to_string(),
                    role: SemanticRole::Button,
                    rect: Rect {
                        x: 900.0,
                        y: 180.0,
                        width: 160.0,
                        height: 48.0,
                    },
                    visible: true,
                    enabled: true,
                    text: Some("Scan now".to_string()),
                    tags: vec!["primary-action".to_string()],
                },
            ],
        }
    }

    #[test]
    fn can_hit_test_and_click_element() {
        let scene = sample_scene();
        let events = click_events_for_target(&scene, &TargetRef::ElementId("scan-now".to_string()))
            .expect("click events");
        assert_eq!(events.len(), 3);
        match &events[1] {
            InputEvent::PointerDown { x, y, .. } => {
                assert!(*x > 900.0);
                assert!(*y > 180.0);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn can_resolve_target_by_tag() {
        let scene = sample_scene();
        let target = resolve_target(&scene, &TargetRef::Tag("hero".to_string())).expect("hero");
        assert_eq!(target.id, "hero");
    }

    #[test]
    fn keypress_yields_down_and_up() {
        let events = keypress_events("Enter");
        assert_eq!(events.len(), 2);
        match &events[0] {
            InputEvent::KeyDown { code, .. } => assert_eq!(code, "Enter"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn uiux_evaluator_flags_missing_runtime_capabilities() {
        let scene = sample_scene();
        let report = evaluate_uiux(
            &scene,
            &RenderCapabilities {
                text_visible: false,
                keyboard_navigation: false,
                focus_ring_visible: false,
                hover_feedback: false,
                responsive_layout: false,
                semantic_lists: false,
            },
        );
        assert!(!report.usable);
        assert!(report.score < 70);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "text.not-rendered")
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "input.keyboard-nav-missing")
        );
    }
}
