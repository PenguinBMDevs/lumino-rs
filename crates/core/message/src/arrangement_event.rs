//! 工程走带交互的**框架无关事件**（UI 无关）
//!
//! 交互层原先直接匹配 旧版 `canvas::Event` / `mouse::Cursor`，导致逻辑无法被
//! egui 复用。这里定义中立事件：各 UI 侧在边界做一次转换，交互层只认中立类型。
//!
//! 只覆盖走带交互真正用到的子集（按键按下/释放、指针移动、滚轮），不做通用事件系统。

use crate::types::geometry::Point2;

/// 指针按键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrangementPointerButton {
    /// 左键。
    Left,
    /// 右键。
    Right,
    /// 中键。
    Middle,
}

/// 滚轮增量（与 旧版 `ScrollDelta` 同形）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArrangementScroll {
    /// 按刻度（一行 = 1.0）。
    Lines {
        /// 横向刻度。
        x: f32,
        /// 纵向刻度。
        y: f32,
    },
    /// 按像素。
    Pixels {
        /// 横向像素。
        x: f32,
        /// 纵向像素。
        y: f32,
    },
}

/// 走带交互事件（框架无关）。
#[derive(Debug, Clone, PartialEq)]
pub enum ArrangementEvent {
    /// 指针按键按下。
    ButtonPressed(ArrangementPointerButton),
    /// 指针按键释放。
    ButtonReleased(ArrangementPointerButton),
    /// 指针移动（坐标见 [`ArrangementCursor::position`]）。
    CursorMoved,
    /// 滚轮滚动。
    WheelScrolled {
        /// 增量。
        delta: ArrangementScroll,
    },
}

impl ArrangementEvent {
    /// 是否与「指针移动」无关（便于交互层判定“无事件”）。
    pub fn is_cursor_moved(&self) -> bool {
        matches!(self, Self::CursorMoved)
    }
}

/// 指针状态（位置 + 是否在界内）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArrangementCursor {
    /// 屏幕坐标；`None` = 指针不在窗口内。
    position: Option<Point2>,
}

impl ArrangementCursor {
    /// 指针在 `pos`。
    pub const fn available(pos: Point2) -> Self {
        Self {
            position: Some(pos),
        }
    }

    /// 指针不在窗口内。
    pub const fn unavailable() -> Self {
        Self { position: None }
    }

    /// 当前屏幕坐标。
    pub const fn position(&self) -> Option<Point2> {
        self.position
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_reports_availability() {
        let cursor = ArrangementCursor::available(Point2::new(3.0, 4.0));
        assert_eq!(cursor.position(), Some(Point2::new(3.0, 4.0)));
        assert_eq!(ArrangementCursor::unavailable().position(), None);
    }

    #[test]
    fn event_predicates() {
        assert!(ArrangementEvent::CursorMoved.is_cursor_moved());
        assert!(!ArrangementEvent::ButtonPressed(ArrangementPointerButton::Left).is_cursor_moved());
    }
}
