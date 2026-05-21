//! Notification system — delegates toast rendering to kami-ui-gpu ToastStack.
//!
//! OS adds consent modal queue on top of the generic toast system.

pub use kami_ui_gpu::{ToastLevel as NotificationLevel, ToastStack};
use serde::{Deserialize, Serialize};

/// A consent request requiring human approval (OS-specific).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRequest {
    /// Unique request ID.
    pub id: u32,
    /// Agent DID requesting consent.
    pub agent_did: String,
    /// Agent display name.
    pub agent_name: String,
    /// Action description.
    pub action: String,
    /// Risk tier: low / medium / high / critical.
    pub risk_tier: String,
    /// Estimated cost in GCC tokens.
    pub estimated_cost: f64,
    /// Additional context JSON.
    pub context_json: String,
}

/// Consent modal response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentResponse {
    Pending,
    Approved,
    Denied,
}

/// Managed notification queue combining generic ToastStack + OS consent modals.
pub struct NotificationQueue {
    /// Generic toast stack (from kami-ui-gpu SDK).
    pub toasts: ToastStack,
    /// Pending consent modals (FIFO, one shown at a time).
    pub consent_queue: Vec<ConsentRequest>,
    /// Consent responses keyed by request ID.
    pub consent_responses: Vec<(u32, ConsentResponse)>,
    /// Next consent request ID.
    next_consent_id: u32,
}

impl NotificationQueue {
    /// Create empty queue.
    pub fn new() -> Self {
        Self {
            toasts: ToastStack::new(),
            consent_queue: Vec::new(),
            consent_responses: Vec::new(),
            next_consent_id: 1,
        }
    }

    /// Push a toast notification (delegates to ToastStack).
    pub fn push_toast(&mut self, title: String, body: String, level: NotificationLevel, duration_ms: u64) {
        self.toasts.push(title, body, level, duration_ms);
    }

    /// Push a consent request. Returns the assigned request ID.
    pub fn push_consent(&mut self, mut request: ConsentRequest) -> u32 {
        let id = self.next_consent_id;
        self.next_consent_id += 1;
        request.id = id;
        self.consent_queue.push(request);
        id
    }

    /// Resolve a consent request.
    pub fn resolve_consent(&mut self, request_id: u32, response: ConsentResponse) {
        self.consent_queue.retain(|r| r.id != request_id);
        self.consent_responses.push((request_id, response));
    }

    /// Get the current active consent modal (front of queue).
    pub fn active_consent(&self) -> Option<&ConsentRequest> {
        self.consent_queue.first()
    }

    /// Tick toast timers + animations.
    pub fn tick(&mut self, dt_ms: u64) {
        self.toasts.tick(dt_ms);
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::new()
    }
}
