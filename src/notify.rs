//! Toast / modal notification queue.
//!
//! Game code calls [`NotifyCenter::push`] from anywhere; the
//! UI layer ticks the queue once per frame and renders whatever
//! [`NotifyCenter::active`] returns. Each notification carries a
//! [`Severity`], a body, and a self-dismissing timer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub severity: Severity,
    pub title: String,
    pub body: String,
    pub remaining_seconds: f32,
    pub modal: bool,
}

impl Notification {
    /// Convenience: short toast that dismisses after 3 seconds.
    #[must_use]
    pub fn toast(severity: Severity, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            severity,
            title: title.into(),
            body: body.into(),
            remaining_seconds: 3.0,
            modal: false,
        }
    }

    /// Convenience: modal that blocks UI until the user dismisses it
    /// (= `remaining_seconds = f32::INFINITY`).
    #[must_use]
    pub fn modal(severity: Severity, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            severity,
            title: title.into(),
            body: body.into(),
            remaining_seconds: f32::INFINITY,
            modal: true,
        }
    }
}

/// Stateful FIFO queue of in-flight notifications. `capacity` caps
/// the number of toasts visible at once; modals are never dropped
/// by the capacity limit.
#[derive(Debug, Clone)]
pub struct NotifyCenter {
    queue: Vec<Notification>,
    pub capacity: usize,
}

impl NotifyCenter {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Vec::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, n: Notification) {
        self.queue.push(n);
        // Drop oldest non-modal toast when over capacity.
        while self.queue.iter().filter(|q| !q.modal).count() > self.capacity {
            if let Some(idx) = self.queue.iter().position(|q| !q.modal) {
                self.queue.remove(idx);
            } else {
                break;
            }
        }
    }

    /// Dismiss the front-most modal in the queue, if any. Toasts are
    /// auto-dismissed by [`Self::tick`].
    pub fn dismiss_modal(&mut self) {
        if let Some(idx) = self.queue.iter().position(|n| n.modal) {
            self.queue.remove(idx);
        }
    }

    /// Decrement every toast's `remaining_seconds` by `dt`, drop the
    /// ones that hit zero. Modals are untouched.
    pub fn tick(&mut self, dt: f32) {
        for n in &mut self.queue {
            if !n.modal {
                n.remaining_seconds = (n.remaining_seconds - dt).max(0.0);
            }
        }
        self.queue.retain(|n| n.modal || n.remaining_seconds > 0.0);
    }

    /// All active notifications, modal first then toasts in arrival
    /// order. Stable across calls when nothing was pushed.
    #[must_use]
    pub fn active(&self) -> Vec<&Notification> {
        let mut out: Vec<&Notification> = self.queue.iter().filter(|n| n.modal).collect();
        out.extend(self.queue.iter().filter(|n| !n.modal));
        out
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl Default for NotifyCenter {
    fn default() -> Self {
        Self::new(4)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_appends_notifications() {
        let mut c = NotifyCenter::new(4);
        c.push(Notification::toast(Severity::Info, "t", "body"));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn capacity_drops_oldest_toast() {
        let mut c = NotifyCenter::new(2);
        for i in 0..5 {
            c.push(Notification::toast(Severity::Info, format!("t{i}"), ""));
        }
        assert_eq!(c.len(), 2);
        assert!(c.active().iter().any(|n| n.title == "t4"));
    }

    #[test]
    fn modal_survives_capacity_limit() {
        let mut c = NotifyCenter::new(1);
        c.push(Notification::modal(Severity::Error, "boom", "body"));
        for i in 0..5 {
            c.push(Notification::toast(Severity::Info, format!("t{i}"), ""));
        }
        let active = c.active();
        assert!(active.iter().any(|n| n.modal && n.title == "boom"));
    }

    #[test]
    fn tick_drops_expired_toasts() {
        let mut c = NotifyCenter::new(4);
        c.push(Notification {
            severity: Severity::Info,
            title: "fast".into(),
            body: String::new(),
            remaining_seconds: 0.5,
            modal: false,
        });
        c.tick(1.0);
        assert!(c.is_empty());
    }

    #[test]
    fn tick_does_not_decrement_modals() {
        let mut c = NotifyCenter::new(4);
        c.push(Notification::modal(Severity::Warning, "wait", "body"));
        c.tick(10.0);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn dismiss_modal_removes_front_modal_only() {
        let mut c = NotifyCenter::new(4);
        c.push(Notification::modal(Severity::Warning, "a", ""));
        c.push(Notification::toast(Severity::Info, "b", ""));
        c.dismiss_modal();
        assert_eq!(c.len(), 1);
        assert!(c.active().iter().all(|n| !n.modal));
    }

    #[test]
    fn active_returns_modals_first() {
        let mut c = NotifyCenter::new(4);
        c.push(Notification::toast(Severity::Info, "toast", ""));
        c.push(Notification::modal(Severity::Error, "modal", ""));
        let active = c.active();
        assert!(active[0].modal);
        assert!(!active[1].modal);
    }
}
