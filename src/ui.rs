//! Retained-mode UI widget system with message-passing architecture.
//!
//! Inspired by fyrox-ui's Control trait + message routing, with a streamlined
//! and designed for declarative widget construction.

use crate::math::{Color, Vec2};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// WidgetId
// ---------------------------------------------------------------------------

/// Handle into the UI widget arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WidgetId(pub u32);

impl WidgetId {
    pub const NONE: Self = Self(u32::MAX);

    #[inline]
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == u32::MAX
    }
}

impl std::fmt::Display for WidgetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_none() {
            write!(f, "Widget(NONE)")
        } else {
            write!(f, "Widget({})", self.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// Axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[inline]
    #[must_use]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }

    #[inline]
    #[must_use]
    pub const fn size(&self) -> Vec2 {
        Vec2::new(self.width, self.height)
    }
}

impl Default for Rect {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Horizontal alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HAlign {
    Left,
    Center,
    Right,
    Stretch,
}

/// Layout direction for child arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LayoutDirection {
    #[default]
    Vertical,
    Horizontal,
}

/// Vertical alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
    Stretch,
}

/// Padding around a widget.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Padding {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Padding {
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    #[must_use]
    pub const fn uniform(v: f32) -> Self {
        Self {
            left: v,
            top: v,
            right: v,
            bottom: v,
        }
    }
}

impl Default for Padding {
    fn default() -> Self {
        Self::ZERO
    }
}

// ---------------------------------------------------------------------------
// Message system
// ---------------------------------------------------------------------------

/// Direction of a UI message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
    /// External code requests a widget change.
    ToWidget,
    /// Widget notifies that a change occurred.
    FromWidget,
}

/// Routing strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    /// Bubble up to ancestors.
    BubbleUp,
    /// Deliver to target only.
    Direct,
}

/// A UI message.
#[derive(Debug, Clone)]
pub struct UiMessage {
    pub target: WidgetId,
    pub direction: MessageDirection,
    pub routing: RoutingStrategy,
    pub payload: MessagePayload,
    pub handled: bool,
}

impl UiMessage {
    #[must_use]
    pub const fn new(
        target: WidgetId,
        direction: MessageDirection,
        payload: MessagePayload,
    ) -> Self {
        Self {
            target,
            direction,
            routing: RoutingStrategy::BubbleUp,
            payload,
            handled: false,
        }
    }
}

/// Payload variants for UI messages.
#[derive(Debug, Clone)]
pub enum MessagePayload {
    Click,
    TextChanged(String),
    ValueChanged(f32),
    CheckChanged(bool),
    FocusGained,
    FocusLost,
    Custom(String),
}

// ---------------------------------------------------------------------------
// WidgetKind — enum dispatch (no trait objects)
// ---------------------------------------------------------------------------

/// Widget kind — static dispatch like fyrox-sound's Effect enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WidgetKind {
    Panel,
    Button { label: String },
    Label { text: String, font_size: f32 },
    TextInput { text: String, placeholder: String },
    Checkbox { checked: bool, label: String },
    Slider { value: f32, min: f32, max: f32 },
    Image { texture_id: u32 },
    ScrollArea { scroll_offset: Vec2 },
    DropdownList { items: Vec<String>, selected: usize },
    ProgressBar { value: f32 },
}

// ---------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------

/// A widget in the retained UI tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub kind: WidgetKind,
    pub rect: Rect,
    pub desired_size: Vec2,
    pub padding: Padding,
    pub h_align: HAlign,
    pub v_align: VAlign,
    pub visible: bool,
    pub enabled: bool,
    pub background: Color,
    pub layout_direction: LayoutDirection,
    pub parent: WidgetId,
    pub children: Vec<WidgetId>,
}

impl Widget {
    #[must_use]
    pub const fn new(kind: WidgetKind) -> Self {
        Self {
            kind,
            rect: Rect::ZERO,
            desired_size: Vec2::ZERO,
            padding: Padding::ZERO,
            h_align: HAlign::Stretch,
            v_align: VAlign::Stretch,
            visible: true,
            enabled: true,
            background: Color::TRANSPARENT,
            layout_direction: LayoutDirection::Vertical,
            parent: WidgetId::NONE,
            children: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// UiContext
// ---------------------------------------------------------------------------

/// The UI widget tree.
pub struct UiContext {
    widgets: Vec<Option<Widget>>,
    free_list: Vec<u32>,
    message_queue: Vec<UiMessage>,
}

impl UiContext {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            widgets: Vec::new(),
            free_list: Vec::new(),
            message_queue: Vec::new(),
        }
    }

    /// Adds a widget and returns its id.
    pub fn add(&mut self, widget: Widget) -> WidgetId {
        if let Some(idx) = self.free_list.pop() {
            self.widgets[idx as usize] = Some(widget);
            WidgetId(idx)
        } else {
            #[allow(clippy::cast_possible_truncation)]
            let idx = self.widgets.len() as u32;
            self.widgets.push(Some(widget));
            WidgetId(idx)
        }
    }

    /// Adds a widget as a child of parent.
    pub fn add_child(&mut self, parent: WidgetId, mut widget: Widget) -> WidgetId {
        widget.parent = parent;
        let child_id = self.add(widget);
        if let Some(Some(p)) = self.widgets.get_mut(parent.0 as usize) {
            p.children.push(child_id);
        }
        child_id
    }

    /// Removes a widget.
    pub fn remove(&mut self, id: WidgetId) -> Option<Widget> {
        let idx = id.0 as usize;
        if idx < self.widgets.len() {
            if let Some(w) = self.widgets[idx].take() {
                if !w.parent.is_none() {
                    if let Some(Some(p)) = self.widgets.get_mut(w.parent.0 as usize) {
                        p.children.retain(|&c| c != id);
                    }
                }
                #[allow(clippy::cast_possible_truncation)]
                self.free_list.push(idx as u32);
                return Some(w);
            }
        }
        None
    }

    /// Returns a reference to a widget.
    #[must_use]
    pub fn get(&self, id: WidgetId) -> Option<&Widget> {
        self.widgets.get(id.0 as usize).and_then(|w| w.as_ref())
    }

    /// Returns a mutable reference to a widget.
    pub fn get_mut(&mut self, id: WidgetId) -> Option<&mut Widget> {
        self.widgets.get_mut(id.0 as usize).and_then(|w| w.as_mut())
    }

    /// Sends a message to the queue.
    pub fn send(&mut self, msg: UiMessage) {
        self.message_queue.push(msg);
    }

    /// Drains the message queue.
    pub fn drain_messages(&mut self) -> Vec<UiMessage> {
        std::mem::take(&mut self.message_queue)
    }

    /// Returns the total number of live widgets.
    #[must_use]
    pub fn widget_count(&self) -> usize {
        self.widgets.iter().filter(|w| w.is_some()).count()
    }

    /// Hit-test: returns the deepest widget containing the point.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<WidgetId> {
        let mut result: Option<(WidgetId, usize)> = None;
        for (i, slot) in self.widgets.iter().enumerate() {
            if let Some(w) = slot {
                if w.visible && w.rect.contains(x, y) {
                    let depth = self.depth(WidgetId(i as u32));
                    if result.is_none() || depth > result.unwrap().1 {
                        result = Some((WidgetId(i as u32), depth));
                    }
                }
            }
        }
        result.map(|(id, _)| id)
    }

    fn depth(&self, id: WidgetId) -> usize {
        let mut d = 0;
        let mut cur = id;
        while let Some(w) = self.get(cur) {
            if w.parent.is_none() {
                break;
            }
            cur = w.parent;
            d += 1;
        }
        d
    }
}

impl Default for UiContext {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Theme / Style
// ---------------------------------------------------------------------------

/// Global UI theme applied to all widgets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTheme {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub font_size: f32,
    pub padding: Padding,
    pub border_radius: f32,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            background: Color::new(0.15, 0.15, 0.15, 1.0),
            foreground: Color::WHITE,
            accent: Color::new(0.2, 0.5, 1.0, 1.0),
            font_size: 14.0,
            padding: Padding::uniform(4.0),
            border_radius: 4.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Focus management
// ---------------------------------------------------------------------------

/// Manages keyboard focus within the UI.
pub struct FocusManager {
    focused: Option<WidgetId>,
    tab_order: Vec<WidgetId>,
}

impl FocusManager {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            focused: None,
            tab_order: Vec::new(),
        }
    }

    /// Returns the currently focused widget.
    #[must_use]
    pub const fn focused(&self) -> Option<WidgetId> {
        self.focused
    }

    /// Sets focus to a specific widget.
    pub const fn set_focus(&mut self, id: WidgetId) {
        self.focused = Some(id);
    }

    /// Clears focus.
    pub const fn clear_focus(&mut self) {
        self.focused = None;
    }

    /// Registers a widget in the tab order.
    pub fn register(&mut self, id: WidgetId) {
        if !self.tab_order.contains(&id) {
            self.tab_order.push(id);
        }
    }

    /// Unregisters a widget from the tab order.
    pub fn unregister(&mut self, id: WidgetId) {
        self.tab_order.retain(|&w| w != id);
        if self.focused == Some(id) {
            self.focused = None;
        }
    }

    /// Moves focus to the next widget in tab order.
    pub fn tab_next(&mut self) {
        if self.tab_order.is_empty() {
            return;
        }
        let current_idx = self
            .focused
            .and_then(|f| self.tab_order.iter().position(|&w| w == f));
        let next = match current_idx {
            Some(i) => (i + 1) % self.tab_order.len(),
            None => 0,
        };
        self.focused = Some(self.tab_order[next]);
    }

    /// Moves focus to the previous widget in tab order.
    pub fn tab_prev(&mut self) {
        if self.tab_order.is_empty() {
            return;
        }
        let current_idx = self
            .focused
            .and_then(|f| self.tab_order.iter().position(|&w| w == f));
        let prev = match current_idx {
            Some(0) | None => self.tab_order.len() - 1,
            Some(i) => i - 1,
        };
        self.focused = Some(self.tab_order[prev]);
    }

    /// Returns the number of widgets in the tab order.
    #[must_use]
    pub const fn tab_order_count(&self) -> usize {
        self.tab_order.len()
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Layout engine — Measure/Arrange two-pass
// ---------------------------------------------------------------------------

impl UiContext {
    /// Runs a simple top-down layout pass on a root widget.
    /// Assigns `rect` to each widget based on parent bounds and alignment.
    pub fn layout(&mut self, root: WidgetId, available: Rect) {
        self.layout_recursive(root, available);
    }

    fn layout_recursive(&mut self, id: WidgetId, available: Rect) {
        let (children, h_align, v_align, padding, desired, direction) = {
            let Some(w) = self.get(id) else { return };
            (
                w.children.clone(),
                w.h_align,
                w.v_align,
                w.padding,
                w.desired_size,
                w.layout_direction,
            )
        };

        let content_x = available.x + padding.left;
        let content_y = available.y + padding.top;
        let content_w = available.width - padding.left - padding.right;
        let content_h = available.height - padding.top - padding.bottom;

        let w = match h_align {
            HAlign::Stretch => content_w,
            _ => desired.x().min(content_w),
        };
        let h = match v_align {
            VAlign::Stretch => content_h,
            _ => desired.y().min(content_h),
        };
        let x = match h_align {
            HAlign::Left | HAlign::Stretch => content_x,
            HAlign::Center => (content_w - w).mul_add(0.5, content_x),
            HAlign::Right => content_x + content_w - w,
        };
        let y = match v_align {
            VAlign::Top | VAlign::Stretch => content_y,
            VAlign::Center => (content_h - h).mul_add(0.5, content_y),
            VAlign::Bottom => content_y + content_h - h,
        };

        if let Some(widget) = self.get_mut(id) {
            widget.rect = Rect::new(x, y, w, h);
        }

        match direction {
            LayoutDirection::Vertical => {
                let mut child_y = y;
                for child_id in children {
                    let child_h = self
                        .get(child_id)
                        .map_or(h, |c| c.desired_size.y())
                        .max(20.0);
                    let child_rect = Rect::new(x, child_y, w, child_h);
                    self.layout_recursive(child_id, child_rect);
                    child_y += child_h;
                }
            }
            LayoutDirection::Horizontal => {
                let mut child_x = x;
                for child_id in children {
                    let child_w = self
                        .get(child_id)
                        .map_or(w, |c| c.desired_size.x())
                        .max(20.0);
                    let child_rect = Rect::new(child_x, y, child_w, h);
                    self.layout_recursive(child_id, child_rect);
                    child_x += child_w;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_id_display() {
        assert_eq!(format!("{}", WidgetId(5)), "Widget(5)");
        assert_eq!(format!("{}", WidgetId::NONE), "Widget(NONE)");
    }

    #[test]
    fn rect_contains() {
        let r = Rect::new(10.0, 10.0, 100.0, 50.0);
        assert!(r.contains(50.0, 30.0));
        assert!(!r.contains(5.0, 5.0));
    }

    #[test]
    fn rect_size() {
        let r = Rect::new(0.0, 0.0, 200.0, 100.0);
        let s = r.size();
        assert_eq!(s.x(), 200.0);
        assert_eq!(s.y(), 100.0);
    }

    #[test]
    fn padding_uniform() {
        let p = Padding::uniform(8.0);
        assert_eq!(p.left, 8.0);
        assert_eq!(p.bottom, 8.0);
    }

    #[test]
    fn ui_add_and_get() {
        let mut ui = UiContext::new();
        let id = ui.add(Widget::new(WidgetKind::Panel));
        assert_eq!(ui.widget_count(), 1);
        assert!(ui.get(id).is_some());
    }

    #[test]
    fn ui_add_child() {
        let mut ui = UiContext::new();
        let parent = ui.add(Widget::new(WidgetKind::Panel));
        let child = ui.add_child(
            parent,
            Widget::new(WidgetKind::Label {
                text: "Hi".to_string(),
                font_size: 14.0,
            }),
        );
        assert_eq!(ui.get(child).unwrap().parent, parent);
        assert!(ui.get(parent).unwrap().children.contains(&child));
    }

    #[test]
    fn ui_remove() {
        let mut ui = UiContext::new();
        let parent = ui.add(Widget::new(WidgetKind::Panel));
        let child = ui.add_child(parent, Widget::new(WidgetKind::Panel));
        ui.remove(child);
        assert!(ui.get(child).is_none());
        assert!(!ui.get(parent).unwrap().children.contains(&child));
    }

    #[test]
    fn ui_message_queue() {
        let mut ui = UiContext::new();
        let id = ui.add(Widget::new(WidgetKind::Button {
            label: "OK".to_string(),
        }));
        ui.send(UiMessage::new(
            id,
            MessageDirection::FromWidget,
            MessagePayload::Click,
        ));
        let msgs = ui.drain_messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].target, id);
        assert!(ui.drain_messages().is_empty());
    }

    #[test]
    fn ui_hit_test() {
        let mut ui = UiContext::new();
        let mut w = Widget::new(WidgetKind::Panel);
        w.rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let id = ui.add(w);
        assert_eq!(ui.hit_test(50.0, 50.0), Some(id));
        assert_eq!(ui.hit_test(200.0, 200.0), None);
    }

    #[test]
    fn ui_hit_test_deepest() {
        let mut ui = UiContext::new();
        let mut parent_w = Widget::new(WidgetKind::Panel);
        parent_w.rect = Rect::new(0.0, 0.0, 200.0, 200.0);
        let parent = ui.add(parent_w);

        let mut child_w = Widget::new(WidgetKind::Button {
            label: "Click".to_string(),
        });
        child_w.rect = Rect::new(10.0, 10.0, 50.0, 50.0);
        let child = ui.add_child(parent, child_w);

        assert_eq!(ui.hit_test(25.0, 25.0), Some(child));
    }

    #[test]
    fn ui_free_list_reuse() {
        let mut ui = UiContext::new();
        let id1 = ui.add(Widget::new(WidgetKind::Panel));
        ui.remove(id1);
        let id2 = ui.add(Widget::new(WidgetKind::Panel));
        assert_eq!(id1.0, id2.0);
    }

    #[test]
    fn widget_kinds() {
        let _ = WidgetKind::Slider {
            value: 0.5,
            min: 0.0,
            max: 1.0,
        };
        let _ = WidgetKind::Checkbox {
            checked: true,
            label: "Enable".to_string(),
        };
        let _ = WidgetKind::TextInput {
            text: String::new(),
            placeholder: "Type here".to_string(),
        };
        let _ = WidgetKind::Image { texture_id: 42 };
        let _ = WidgetKind::ScrollArea {
            scroll_offset: Vec2::ZERO,
        };
        let _ = WidgetKind::DropdownList {
            items: vec!["A".to_string(), "B".to_string()],
            selected: 0,
        };
        let _ = WidgetKind::ProgressBar { value: 0.75 };
    }

    #[test]
    fn message_direction() {
        let msg = UiMessage::new(
            WidgetId(0),
            MessageDirection::ToWidget,
            MessagePayload::ValueChanged(0.5),
        );
        assert_eq!(msg.direction, MessageDirection::ToWidget);
        assert!(!msg.handled);
    }

    #[test]
    fn invisible_widget_not_hit() {
        let mut ui = UiContext::new();
        let mut w = Widget::new(WidgetKind::Panel);
        w.rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        w.visible = false;
        ui.add(w);
        assert_eq!(ui.hit_test(50.0, 50.0), None);
    }

    #[test]
    fn depth_calculation() {
        let mut ui = UiContext::new();
        let root = ui.add(Widget::new(WidgetKind::Panel));
        let child = ui.add_child(root, Widget::new(WidgetKind::Panel));
        let grandchild = ui.add_child(child, Widget::new(WidgetKind::Panel));
        assert_eq!(ui.depth(root), 0);
        assert_eq!(ui.depth(child), 1);
        assert_eq!(ui.depth(grandchild), 2);
    }

    #[test]
    fn theme_default() {
        let theme = UiTheme::default();
        assert_eq!(theme.font_size, 14.0);
        assert_eq!(theme.border_radius, 4.0);
    }

    #[test]
    fn focus_manager_basic() {
        let mut fm = FocusManager::new();
        let a = WidgetId(0);
        let b = WidgetId(1);
        fm.register(a);
        fm.register(b);
        assert_eq!(fm.focused(), None);
        fm.set_focus(a);
        assert_eq!(fm.focused(), Some(a));
    }

    #[test]
    fn focus_tab_next() {
        let mut fm = FocusManager::new();
        let a = WidgetId(0);
        let b = WidgetId(1);
        let c = WidgetId(2);
        fm.register(a);
        fm.register(b);
        fm.register(c);
        fm.tab_next();
        assert_eq!(fm.focused(), Some(a));
        fm.tab_next();
        assert_eq!(fm.focused(), Some(b));
        fm.tab_next();
        assert_eq!(fm.focused(), Some(c));
        fm.tab_next();
        assert_eq!(fm.focused(), Some(a)); // wraps
    }

    #[test]
    fn focus_tab_prev() {
        let mut fm = FocusManager::new();
        let a = WidgetId(0);
        let b = WidgetId(1);
        fm.register(a);
        fm.register(b);
        fm.set_focus(a);
        fm.tab_prev();
        assert_eq!(fm.focused(), Some(b)); // wraps
    }

    #[test]
    fn focus_unregister() {
        let mut fm = FocusManager::new();
        let a = WidgetId(0);
        fm.register(a);
        fm.set_focus(a);
        fm.unregister(a);
        assert_eq!(fm.focused(), None);
        assert_eq!(fm.tab_order_count(), 0);
    }

    #[test]
    fn focus_clear() {
        let mut fm = FocusManager::new();
        fm.set_focus(WidgetId(0));
        fm.clear_focus();
        assert_eq!(fm.focused(), None);
    }

    #[test]
    fn layout_stretch() {
        let mut ui = UiContext::new();
        let root = ui.add(Widget::new(WidgetKind::Panel));
        ui.layout(root, Rect::new(0.0, 0.0, 800.0, 600.0));
        let r = ui.get(root).unwrap().rect;
        assert!((r.width - 800.0).abs() < 1e-3);
        assert!((r.height - 600.0).abs() < 1e-3);
    }

    #[test]
    fn layout_with_padding() {
        let mut ui = UiContext::new();
        let mut w = Widget::new(WidgetKind::Panel);
        w.padding = Padding::uniform(10.0);
        let root = ui.add(w);
        let child = ui.add_child(root, Widget::new(WidgetKind::Panel));
        ui.layout(root, Rect::new(0.0, 0.0, 200.0, 100.0));
        let child_rect = ui.get(child).unwrap().rect;
        assert!((child_rect.x - 10.0).abs() < 1e-3);
    }

    #[test]
    fn layout_center_align() {
        let mut ui = UiContext::new();
        let mut w = Widget::new(WidgetKind::Button {
            label: "OK".to_string(),
        });
        w.h_align = HAlign::Center;
        w.v_align = VAlign::Center;
        w.desired_size = Vec2::new(100.0, 40.0);
        let id = ui.add(w);
        ui.layout(id, Rect::new(0.0, 0.0, 400.0, 300.0));
        let r = ui.get(id).unwrap().rect;
        assert!((r.x - 150.0).abs() < 1e-3);
        assert!((r.y - 130.0).abs() < 1e-3);
    }

    #[test]
    fn layout_right_align() {
        let mut ui = UiContext::new();
        let mut w = Widget::new(WidgetKind::Panel);
        w.h_align = HAlign::Right;
        w.desired_size = Vec2::new(50.0, 50.0);
        let id = ui.add(w);
        ui.layout(id, Rect::new(0.0, 0.0, 200.0, 100.0));
        let r = ui.get(id).unwrap().rect;
        assert!((r.x - 150.0).abs() < 1e-3);
    }

    #[test]
    fn layout_children_stacked() {
        let mut ui = UiContext::new();
        let root = ui.add(Widget::new(WidgetKind::Panel));
        let mut c1 = Widget::new(WidgetKind::Panel);
        c1.desired_size = Vec2::new(100.0, 30.0);
        let c1_id = ui.add_child(root, c1);
        let mut c2 = Widget::new(WidgetKind::Panel);
        c2.desired_size = Vec2::new(100.0, 40.0);
        let c2_id = ui.add_child(root, c2);
        ui.layout(root, Rect::new(0.0, 0.0, 200.0, 200.0));
        let r1 = ui.get(c1_id).unwrap().rect;
        let r2 = ui.get(c2_id).unwrap().rect;
        assert!((r2.y - (r1.y + 30.0)).abs() < 1e-3);
    }

    #[test]
    fn focus_empty_tab() {
        let mut fm = FocusManager::new();
        fm.tab_next(); // Should not panic
        assert_eq!(fm.focused(), None);
    }

    #[test]
    fn focus_duplicate_register() {
        let mut fm = FocusManager::new();
        let a = WidgetId(0);
        fm.register(a);
        fm.register(a);
        assert_eq!(fm.tab_order_count(), 1);
    }

    #[test]
    fn layout_horizontal() {
        let mut ui = UiContext::new();
        let mut root = Widget::new(WidgetKind::Panel);
        root.layout_direction = LayoutDirection::Horizontal;
        let root_id = ui.add(root);
        let mut c1 = Widget::new(WidgetKind::Panel);
        c1.desired_size = Vec2::new(50.0, 30.0);
        let c1_id = ui.add_child(root_id, c1);
        let mut c2 = Widget::new(WidgetKind::Panel);
        c2.desired_size = Vec2::new(80.0, 30.0);
        let c2_id = ui.add_child(root_id, c2);
        ui.layout(root_id, Rect::new(0.0, 0.0, 400.0, 100.0));
        let r1 = ui.get(c1_id).unwrap().rect;
        let r2 = ui.get(c2_id).unwrap().rect;
        assert!((r1.x).abs() < 1e-3);
        assert!((r2.x - 50.0).abs() < 1e-3);
    }

    #[test]
    fn layout_direction_default_vertical() {
        let w = Widget::new(WidgetKind::Panel);
        assert_eq!(w.layout_direction, LayoutDirection::Vertical);
    }
}
